//! `ds` — domain search: a simple domain availability checker.
//!
//! RDAP first (IANA bootstrap), WHOIS fallback for TLDs with no RDAP service,
//! using the server/needle table in the bundled `whois.json`.

mod bootstrap;
mod dns;
mod limit;
mod model;
mod pricing;
mod private;
mod rdap;
mod registration;
#[cfg(feature = "serve")]
mod serve;
mod tlds;
mod whois;

use std::collections::BTreeSet;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use clap::{Parser, ValueEnum};
use futures::stream::StreamExt;
use hickory_resolver::TokioAsyncResolver;

use bootstrap::Bootstrap;
use limit::HostLimiter;
use model::{CheckResult, Details, Method, Register, Status};
use pricing::Prices;
use private::PrivateTlds;
use registration::Rules;
use tlds::Registry;
use whois::TldInfo;

const USER_AGENT: &str = concat!("ds/", env!("CARGO_PKG_VERSION"), " (domain-search)");

#[derive(Parser, Debug)]
#[command(
    name = "ds",
    version,
    disable_version_flag = true,
    about = "Check domain availability over RDAP with a WHOIS fallback",
    long_about = "Check domain availability over RDAP with a WHOIS fallback.\n\n\
                  Examples:\n  \
                  ds apple --tld all\n  \
                  ds apple --tld com,net --details\n  \
                  ds apple,orange,bangla,english --tld com,net\n  \
                  ds @names.txt --tld popular --available-only --save\n  \
                  ds apple --tld all --level second\n  \
                  ds apple --tld com,io --where\n  \
                  ds apple --tld all --private only\n  \
                  ds apple google --tld popular --whois --dns-records\n  \
                  ds apple.com --details --registry\n  \
                  ds --serve --port 8080"
)]
struct Args {
    /// Serve the checks over HTTP instead of checking a name. See the
    /// "server options" below for the address, the limits and the cache.
    #[cfg(feature = "serve")]
    #[arg(long)]
    serve: bool,

    /// Name(s) to check: `apple`, `apple,orange,bangla`, `@names.txt`, or
    /// several arguments. A full domain (`apple.com`) is checked as-is when
    /// --tld is omitted.
    #[cfg_attr(feature = "serve", arg(required_unless_present = "serve"))]
    #[cfg_attr(not(feature = "serve"), arg(required = true))]
    names: Vec<String>,

    /// TLDs to check: comma separated list, `all`, `popular`, `rdap`, or `@file`.
    #[arg(short, long)]
    tld: Option<String>,

    /// Query WHOIS as well and print the raw response.
    #[arg(long)]
    whois: bool,

    /// Show registration details (registrar, dates, status, nameservers).
    #[arg(long)]
    details: bool,

    /// Show which registry answered (RDAP endpoint / WHOIS server).
    #[arg(long)]
    registry: bool,

    /// Look up A/AAAA/NS/MX/TXT/CNAME/SOA records.
    #[arg(long = "dns-records")]
    dns_records: bool,

    /// For available domains, show the registry, any restriction on who may
    /// register, anything the TLD requires of the name, and the registrars
    /// that sell the TLD.
    #[arg(long = "where")]
    show_where: bool,

    /// Shorthand for --details --registry --whois --dns-records --where.
    #[arg(long = "all-info")]
    all_info: bool,

    /// Include the raw RDAP JSON in the output.
    #[arg(long)]
    raw: bool,

    /// Print results as a JSON array instead of text.
    #[arg(long)]
    json: bool,

    /// Only print available domains. Saved files still cover every result.
    #[arg(long = "available-only")]
    available_only: bool,

    /// Parallel lookups.
    #[arg(short, long, default_value_t = 20)]
    concurrency: usize,

    /// Parallel lookups against any single registry server.
    #[arg(long = "per-host", default_value_t = 4)]
    per_host: usize,

    /// Per-request timeout in seconds.
    #[arg(long, default_value_t = 10)]
    timeout: u64,

    /// Write the results to available.txt, unavailable.txt and, if any
    /// lookup failed, unknown.txt. With --json they are written as
    /// available.json, unavailable.json and unknown.json instead. Nothing is
    /// written without this.
    #[arg(short, long)]
    save: bool,

    /// Directory for those files (implies --save). Defaults to the working
    /// directory.
    #[arg(short, long, value_name = "DIR")]
    out_dir: Option<PathBuf>,

    /// Append to the output files instead of overwriting them (implies --save).
    #[arg(long)]
    append: bool,

    /// Which level the name is registered at: second (`apple.com`), third
    /// (`apple.co.uk`), or any. Filters the --tld list.
    #[arg(long, value_enum, default_value_t = Level::Any)]
    level: Level,

    /// Keep only TLDs of this length: `2` for the two-letter ccTLDs, or a
    /// range like `2-3`, `-3`, `4-`. Measured on the last label, so `co.uk`
    /// counts as 2.
    #[arg(long = "tld-len", value_name = "N|MIN-MAX", allow_hyphen_values = true)]
    tld_len: Option<String>,

    /// Only country-code TLDs — the two-letter ones (`de`, `io`, `co.uk`).
    /// Shorthand for --tld-len 2.
    #[arg(long)]
    cctld: bool,

    /// Brand and reserved TLDs, where the public cannot register at all
    /// (`aws`, `google`, `arpa`): exclude them, include them, or check only
    /// those. They are excluded from --tld all/rdap/popular unless you say
    /// otherwise; a TLD you name yourself is always checked.
    #[arg(long = "private", value_enum, value_name = "MODE")]
    private: Option<PrivateMode>,

    /// Where to look: auto (RDAP, then the bundled WHOIS table, then an IANA
    /// referral), rdap (RDAP only), or whois (bundled whois.json only).
    #[arg(long, value_enum, default_value_t = Source::Auto)]
    source: Source,

    /// Custom RDAP server list, in the bootstrap format IANA publishes.
    /// Without this, ./rdap.json and ~/.config/ds/rdap.json are picked up
    /// automatically when they exist.
    #[arg(long = "rdap-file", value_name = "PATH")]
    rdap_file: Option<PathBuf>,

    /// What to do with that list: merge it over the IANA bootstrap (custom
    /// entries win) or use only it.
    #[arg(long = "rdap-mode", value_enum, default_value_t = ListMode::Merge)]
    rdap_mode: ListMode,

    /// Custom WHOIS server table, in the same format as the bundled
    /// whois.json. Without this, ./whois.json and ~/.config/ds/whois.json are
    /// picked up automatically when they exist.
    #[arg(long = "whois-file", value_name = "PATH")]
    whois_file: Option<PathBuf>,

    /// What to do with that table: merge it over the bundled one (custom
    /// entries win) or use only it.
    #[arg(long = "whois-mode", value_enum, default_value_t = ListMode::Merge)]
    whois_mode: ListMode,

    /// Custom price table, in the same format as the bundled pricing.json.
    /// Without this, ./pricing.json and ~/.config/ds/pricing.json are picked
    /// up automatically when they exist.
    #[arg(long = "pricing-file", value_name = "PATH")]
    pricing_file: Option<PathBuf>,

    /// What to do with that table: merge it over the bundled one (custom
    /// entries win) or use only it.
    #[arg(long = "pricing-mode", value_enum, default_value_t = ListMode::Merge)]
    pricing_mode: ListMode,

    /// Never query whois.iana.org: no referral when a bundled WHOIS host is
    /// stale, and no registry names for --where.
    #[arg(long = "no-iana")]
    no_iana: bool,

    /// Re-download the IANA RDAP bootstrap file.
    #[arg(long = "refresh")]
    refresh: bool,

    /// Suppress progress output; only the summary is printed.
    #[arg(short, long)]
    quiet: bool,

    /// Disable coloured output.
    #[arg(long = "no-color")]
    no_color: bool,

    /// Print the version and exit.
    #[arg(short = 'v', short_alias = 'V', long, action = clap::ArgAction::Version)]
    version: Option<bool>,

    /// The options that only mean anything with --serve. What the server
    /// shares with the CLI — --concurrency, --per-host, --timeout, --source,
    /// --no-iana, --details, --registry, --where — it reads off the fields
    /// above instead, so a flag means the same thing on both sides.
    #[cfg(feature = "serve")]
    #[command(flatten)]
    serve_opts: serve::ServeArgs,
}

/// Which level of the tree a name is registered at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Level {
    /// Both.
    Any,
    /// Straight under the TLD: `apple.com`, `apple.de`.
    Second,
    /// Under a multi-label suffix: `apple.co.uk`, `apple.com.au`.
    Third,
}

impl Level {
    /// `com` is a second-level registration, `co.uk` a third-level one.
    fn allows(self, tld: &str) -> bool {
        match self {
            Level::Any => true,
            Level::Second => !tld.contains('.'),
            Level::Third => tld.contains('.'),
        }
    }
}

/// An inclusive length filter for TLDs: `2`, `2-3`, `-3`, `4-`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LenRange {
    min: usize,
    max: usize,
}

impl LenRange {
    fn parse(spec: &str) -> Result<Self> {
        let spec = spec.trim();
        let bad = || anyhow::anyhow!("--tld-len wants `2`, `2-3`, `-3` or `4-`, not `{spec}`");

        let (min, max) = match spec.split_once('-') {
            None => {
                let n: usize = spec.parse().map_err(|_| bad())?;
                (n, n)
            }
            Some((lo, hi)) => {
                let min = if lo.trim().is_empty() {
                    1
                } else {
                    lo.trim().parse().map_err(|_| bad())?
                };
                let max = if hi.trim().is_empty() {
                    usize::MAX
                } else {
                    hi.trim().parse().map_err(|_| bad())?
                };
                (min, max)
            }
        };

        if min == 0 || min > max {
            return Err(bad());
        }
        Ok(Self { min, max })
    }

    /// Measured on the last label: the TLD of `co.uk` is `uk`.
    fn allows(&self, tld: &str) -> bool {
        let apex = tld.rsplit('.').next().unwrap_or(tld);
        (self.min..=self.max).contains(&apex.chars().count())
    }
}

/// What to do with TLDs the public cannot register in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum PrivateMode {
    /// Drop them from the TLD list. The default for `all`, `rdap` and
    /// `popular`.
    Exclude,
    /// Keep them, still marked PRIVATE in the results.
    Include,
    /// Check nothing else — useful for seeing what a sweep is skipping.
    Only,
}

/// How a custom server list relates to the built-in one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ListMode {
    /// Custom entries win, everything else comes from the built-in list.
    Merge,
    /// Use the custom list alone.
    Only,
}

/// Which lookup source a run is allowed to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
enum Source {
    /// RDAP, then the bundled WHOIS table, then an IANA referral.
    #[default]
    Auto,
    /// RDAP only.
    Rdap,
    /// The bundled whois.json table only.
    Whois,
}

static COLOR: AtomicBool = AtomicBool::new(false);

/// The flags that change what a single lookup does, split out of `Args` so
/// that something other than the CLI — `ds serve` — can drive `check` without
/// having to fabricate a whole command line.
#[derive(Debug, Clone, Copy, Default)]
struct Lookup {
    source: Source,
    whois: bool,
    details: bool,
    registry: bool,
    show_where: bool,
    no_iana: bool,
    raw: bool,
    json: bool,
}

impl From<&Args> for Lookup {
    fn from(a: &Args) -> Self {
        Self {
            source: a.source,
            whois: a.whois,
            details: a.details,
            registry: a.registry,
            show_where: a.show_where,
            no_iana: a.no_iana,
            raw: a.raw,
            json: a.json,
        }
    }
}

struct Ctx {
    opts: Lookup,
    client: reqwest::Client,
    limiter: HostLimiter,
    bootstrap: Bootstrap,
    registry: Registry,
    prices: Prices,
    /// TLDs with no public registrations, so an empty answer is not an offer.
    private: PrivateTlds,
    /// Registration restrictions, for --where.
    rules: Rules,
    /// Used for every RDAP/WHOIS connection.
    resolver: TokioAsyncResolver,
    /// Only for `--dns-records`.
    records: Option<TokioAsyncResolver>,
    timeout: Duration,
    /// TLD -> what IANA says about it. One lookup per TLD, failures cached
    /// too: IANA's WHOIS rate-limits hard, and asking it again for every
    /// domain under the same TLD only makes that worse.
    tld_info: tokio::sync::Mutex<std::collections::HashMap<String, Option<Arc<TldInfo>>>>,
}

impl Ctx {
    async fn tld_info(&self, tld: &str) -> Option<Arc<TldInfo>> {
        let apex = tld.rsplit('.').next().unwrap_or(tld).to_string();
        if let Some(cached) = self.tld_info.lock().await.get(&apex) {
            return cached.clone();
        }
        let info = whois::iana_tld_info(&self.resolver, &self.limiter, &apex, self.timeout)
            .await
            .ok()
            .map(Arc::new);
        self.tld_info.lock().await.insert(apex, info.clone());
        info
    }
}

#[tokio::main]
async fn main() {
    // Rust ignores SIGPIPE, which turns `ds ... | head` into a panic on the
    // first write after the reader exits. Behave like every other CLI.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    if let Err(e) = run().await {
        eprintln!("{} {e:#}", paint("error:", Color::Red));
        std::process::exit(2);
    }
}

async fn run() -> Result<()> {
    let mut args = Args::parse();

    // The server takes the process over, so it is dispatched before any of the
    // CLI's own setup runs.
    #[cfg(feature = "serve")]
    if args.serve {
        return serve::run(args).await;
    }

    if args.all_info {
        args.details = true;
        args.registry = true;
        args.whois = true;
        args.dns_records = true;
        args.show_where = true;
    }
    if args.cctld && args.tld_len.is_none() {
        // Two letters is exactly the ccTLD space: ICANN reserves them for
        // ISO 3166-1 country codes.
        args.tld_len = Some("2".into());
    }
    if args.concurrency == 0 {
        args.concurrency = 1;
    }
    COLOR.store(
        !args.no_color
            && std::env::var_os("NO_COLOR").is_none()
            && std::io::stdout().is_terminal()
            && enable_ansi(),
        Ordering::Relaxed,
    );

    let timeout = Duration::from_secs(args.timeout.max(1));
    let resolver = dns::connect_resolver(timeout);
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .connect_timeout(timeout)
        .dns_resolver(dns::Dns::new(&resolver))
        .build()
        .context("building HTTP client")?;

    let whois_path = args
        .whois_file
        .clone()
        .or_else(|| discover_config("whois.json"));
    let custom_whois = match &whois_path {
        Some(path) => Some(Registry::from_file(path)?),
        None => None,
    };
    let whois_only = custom_whois.is_some() && args.whois_mode == ListMode::Only;

    let mut registry = if whois_only {
        Registry::default()
    } else {
        Registry::load().context("loading the bundled whois.json")?
    };
    if let (Some(custom), Some(path)) = (custom_whois, &whois_path) {
        let count = custom.len();
        registry.merge(custom);
        list_note("whois:", count, path, whois_only, &args);
    }

    let pricing_path = args
        .pricing_file
        .clone()
        .or_else(|| discover_config("pricing.json"));
    let custom_prices = match &pricing_path {
        Some(path) => Some(Prices::from_file(path)?),
        None => None,
    };
    let prices_only = custom_prices.is_some() && args.pricing_mode == ListMode::Only;

    let mut prices = if prices_only {
        Prices::default()
    } else {
        Prices::load().context("loading the bundled pricing.json")?
    };
    if let (Some(custom), Some(path)) = (custom_prices, &pricing_path) {
        let count = custom.len();
        prices.merge(custom);
        list_note("pricing:", count, path, prices_only, &args);
    }

    let custom_path = args
        .rdap_file
        .clone()
        .or_else(|| discover_config("rdap.json"));
    let custom = match &custom_path {
        Some(path) => Some(Bootstrap::from_file(path)?),
        None => None,
    };
    let custom_only = custom.is_some() && args.rdap_mode == ListMode::Only;

    let mut bootstrap = if args.source == Source::Whois || custom_only {
        Bootstrap::default()
    } else {
        match Bootstrap::load(&client, args.refresh).await {
            Ok(b) => b,
            Err(e) => {
                eprintln!(
                    "{} RDAP bootstrap unavailable ({e:#}); falling back to WHOIS",
                    paint("warning:", Color::Yellow)
                );
                Bootstrap::default()
            }
        }
    };

    if let (Some(custom), Some(path)) = (custom, &custom_path) {
        let count = custom.len();
        bootstrap.merge(custom);
        list_note("rdap:", count, path, custom_only, &args);
    }
    if bootstrap.is_empty() && args.source == Source::Rdap {
        bail!("no RDAP bootstrap data and --source rdap was given: nothing to query with");
    }
    if args.whois && args.source == Source::Rdap {
        eprintln!(
            "{} --whois is ignored with --source rdap",
            paint("warning:", Color::Yellow)
        );
    }

    let records = args.dns_records.then(|| dns::resolver(timeout));

    let private_tlds = PrivateTlds::load().context("loading the bundled private-tlds.json")?;

    let targets = build_targets(&args, &registry, &bootstrap, &private_tlds)?;
    if targets.is_empty() {
        bail!("no domains to check");
    }

    if !args.quiet && !args.json {
        println!(
            "Checking {} domain{} ({} concurrent)\n",
            paint(&targets.len().to_string(), Color::Bold),
            if targets.len() == 1 { "" } else { "s" },
            args.concurrency
        );
    }

    let concurrency = args.concurrency;
    let quiet = args.quiet;
    let json = args.json;
    let available_only = args.available_only;

    let ctx = Arc::new(Ctx {
        limiter: HostLimiter::new(args.per_host),
        opts: Lookup::from(&args),
        client,
        bootstrap,
        registry,
        prices,
        private: private_tlds,
        rules: Rules::load().context("loading the bundled eligibility.json")?,
        resolver,
        records,
        timeout,
        tld_info: tokio::sync::Mutex::new(std::collections::HashMap::new()),
    });

    let started = Instant::now();
    let mut results: Vec<(usize, CheckResult)> = Vec::with_capacity(targets.len());

    let mut stream = futures::stream::iter(targets.into_iter().enumerate().map(|(i, t)| {
        let ctx = Arc::clone(&ctx);
        async move { (i, check(&ctx, t.domain, t.tld).await) }
    }))
    .buffer_unordered(concurrency);

    while let Some((i, result)) = stream.next().await {
        if !quiet && !json && !(available_only && result.status != Status::Available) {
            print_result(&args, &result);
        }
        results.push((i, result));
    }

    results.sort_by_key(|(i, _)| *i);
    let results: Vec<CheckResult> = results.into_iter().map(|(_, r)| r).collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    }

    // Saving is opt-in; asking for a directory or an append is asking to save.
    let written = if args.save || args.out_dir.is_some() || args.append {
        save(&args, &results)?
    } else {
        Vec::new()
    };

    if !json {
        summarize(&results, started.elapsed(), &written);
    }

    let available = results
        .iter()
        .filter(|r| r.status == Status::Available)
        .count();
    if available == 0 {
        std::process::exit(1);
    }
    Ok(())
}

struct Target {
    domain: String,
    tld: String,
}

fn build_targets(
    args: &Args,
    registry: &Registry,
    bootstrap: &Bootstrap,
    private_tlds: &PrivateTlds,
) -> Result<Vec<Target>> {
    let tld_list: Option<Vec<String>> = match &args.tld {
        Some(spec) => {
            let Selection { tlds: all, keyword } = resolve_tlds(spec, registry, bootstrap)?;
            let len = args.tld_len.as_deref().map(LenRange::parse).transpose()?;
            // Naming a TLD is a request to check it, so the default exclusion
            // only prunes the lists `ds` chose itself. An explicit --private
            // filters either way.
            let private = match args.private {
                Some(mode) => Some(mode),
                None if keyword => Some(PrivateMode::Exclude),
                None => None,
            };
            let mut dropped = 0usize;
            let kept: Vec<String> = all
                .iter()
                .filter(|t| args.level.allows(t))
                .filter(|t| len.is_none_or(|l| l.allows(t)))
                .filter(|t| match private {
                    Some(PrivateMode::Exclude) if private_tlds.contains(t) => {
                        dropped += 1;
                        false
                    }
                    Some(PrivateMode::Only) => private_tlds.contains(t),
                    Some(PrivateMode::Exclude) | Some(PrivateMode::Include) | None => true,
                })
                .cloned()
                .collect();
            // A quieter list than last release deserves an explanation, and
            // the flag that undoes it should be one line away.
            if dropped > 0 && args.private.is_none() && !args.quiet && !args.json {
                println!(
                    "{} {dropped} brand/reserved TLD{} skipped ({} to check them too)",
                    paint("private:", Color::Dim),
                    if dropped == 1 { "" } else { "s" },
                    paint("--private include", Color::Bold)
                );
            }
            if kept.is_empty() {
                bail!(
                    "nothing left to check: all {} TLDs were dropped by \
                     --level/--tld-len/--private",
                    all.len()
                );
            }
            Some(kept)
        }
        // A full domain typed out by hand is checked as given.
        None => None,
    };

    let mut targets = Vec::new();
    let mut seen = BTreeSet::new();

    for name in expand_names(&args.names)? {
        let domains: Vec<String> = match &tld_list {
            // No --tld: a dotted name is a full domain, a bare label defaults to .com
            None if name.contains('.') => vec![name.clone()],
            None => vec![format!("{name}.com")],
            Some(tlds) => tlds.iter().map(|t| format!("{name}.{t}")).collect(),
        };

        for domain in domains {
            let ascii = idna::domain_to_ascii(&domain)
                .map_err(|e| anyhow::anyhow!("invalid domain {domain}: {e}"))?;
            if !seen.insert(ascii.clone()) {
                continue;
            }
            let tld = ascii
                .split_once('.')
                .map(|(_, t)| t.to_string())
                .unwrap_or_default();
            targets.push(Target { domain: ascii, tld });
        }
    }

    Ok(targets)
}

/// Look for a config file the user did not name explicitly: first in the
/// working directory, then in the config directory.
fn discover_config(name: &str) -> Option<PathBuf> {
    let cwd = PathBuf::from(name);
    if cwd.is_file() {
        return Some(cwd);
    }

    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?
        .join("ds")
        .join(name);

    config.is_file().then_some(config)
}

/// Say which custom list was loaded, so a stray file in the working directory
/// cannot quietly change the results.
fn list_note(kind: &str, count: usize, path: &Path, only: bool, args: &Args) {
    if args.quiet || args.json {
        return;
    }
    println!(
        "{} {} TLD{} from {} ({})",
        paint(kind, Color::Dim),
        count,
        if count == 1 { "" } else { "s" },
        path.display(),
        if only { "only" } else { "merged" }
    );
}

/// Expand the positional arguments into the list of names to check.
///
/// Each argument may be a single name, a comma separated list, or `@file`
/// (one name per line; commas and `#` comments allowed).
fn expand_names(args: &[String]) -> Result<Vec<String>> {
    let mut names = Vec::new();

    for arg in args {
        let raw = match arg.strip_prefix('@') {
            Some(path) => std::fs::read_to_string(path)
                .with_context(|| format!("reading names from {path}"))?,
            None => arg.clone(),
        };

        for line in raw.lines() {
            let line = line.split('#').next().unwrap_or("");
            for name in line.split(',') {
                let name = name.trim().trim_matches('.').to_ascii_lowercase();
                if !name.is_empty() {
                    names.push(name);
                }
            }
        }
    }

    if names.is_empty() {
        bail!("no names to check");
    }
    Ok(dedup(names))
}

/// What a `--tld` spec expanded to, and whether `ds` chose the members itself
/// (`all`, `rdap`, `popular`) rather than the user listing them.
struct Selection {
    tlds: Vec<String>,
    keyword: bool,
}

fn resolve_tlds(spec: &str, registry: &Registry, bootstrap: &Bootstrap) -> Result<Selection> {
    let spec = spec.trim();
    let listed = |tlds| Selection {
        tlds,
        keyword: false,
    };

    if let Some(path) = spec.strip_prefix('@') {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading TLD list from {path}"))?;
        return Ok(listed(dedup(
            text.lines()
                .map(|l| l.split('#').next().unwrap_or("").to_string())
                .flat_map(|l| {
                    l.split(',')
                        .map(tlds::normalize_tld)
                        .filter(|t| !t.is_empty())
                        .collect::<Vec<_>>()
                })
                .collect(),
        )));
    }

    let keyword = |tlds| Selection {
        tlds,
        keyword: true,
    };

    match spec.to_ascii_lowercase().as_str() {
        "all" => {
            let mut set = registry.all_tlds();
            set.extend(bootstrap.all_tlds());
            Ok(keyword(set.into_iter().collect()))
        }
        "rdap" => Ok(keyword(bootstrap.all_tlds().into_iter().collect())),
        "popular" => Ok(keyword(
            tlds::POPULAR.iter().map(|t| t.to_string()).collect(),
        )),
        _ => {
            let list = dedup(
                spec.split(',')
                    .map(tlds::normalize_tld)
                    .filter(|t| !t.is_empty())
                    .collect(),
            );
            if list.is_empty() {
                bail!("--tld did not contain any usable TLD");
            }
            Ok(listed(list))
        }
    }
}

fn dedup(items: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    items
        .into_iter()
        .filter(|i| seen.insert(i.clone()))
        .collect()
}

async fn check(ctx: &Ctx, domain: String, tld: String) -> CheckResult {
    let started = Instant::now();
    let args = &ctx.opts;

    let mut status = Status::Unknown;
    let mut method = Method::None;
    let mut registry_label = None;
    let mut whois_server = None;
    let mut notes: Vec<String> = Vec::new();
    let mut details: Option<Details> = None;
    let mut rdap_raw = None;
    let mut whois_raw = None;

    // 1. RDAP
    if args.source != Source::Whois {
        match ctx.bootstrap.servers_for(&tld) {
            Some(servers) => {
                match rdap::query(
                    &ctx.client,
                    &ctx.limiter,
                    servers,
                    &domain,
                    args.raw || args.json,
                )
                .await
                {
                    Ok(resp) => {
                        status = if resp.available {
                            Status::Available
                        } else {
                            Status::Taken
                        };
                        method = Method::Rdap;
                        registry_label = Some(resp.server);
                        details = resp.details;
                        rdap_raw = if args.raw { resp.raw } else { None };
                    }
                    Err(e) => notes.push(format!("rdap: {e:#}")),
                }
            }
            None => notes.push("no RDAP service for this TLD".into()),
        }
    }

    // 2. WHOIS — as a fallback, or because the user asked for the raw record.
    //    The bundled server is tried first; if it is gone or says nothing
    //    useful, IANA is asked who serves the TLD today.
    let need_whois = args.source != Source::Rdap && (status == Status::Unknown || args.whois);
    if need_whois {
        let bundled = ctx.registry.lookup(&tld).cloned();
        let mut settled = false;

        if let Some(server) = &bundled {
            match whois::query(
                &ctx.client,
                &ctx.resolver,
                &ctx.limiter,
                server,
                &domain,
                ctx.timeout,
            )
            .await
            {
                Ok(resp) => {
                    settled = resp.status != Status::Unknown;
                    whois_server = Some(resp.server.clone());
                    if status == Status::Unknown && settled {
                        status = resp.status;
                        method = Method::Whois;
                    }
                    if let Some(n) = &resp.note {
                        notes.push(format!("whois: {n}"));
                    }
                    if details.as_ref().is_none_or(Details::is_empty)
                        && resp.status == Status::Taken
                    {
                        let parsed = whois::parse_details(&resp.raw);
                        if !parsed.is_empty() {
                            details = Some(parsed);
                        }
                    }
                    if args.whois {
                        whois_raw = Some(resp.raw.trim().to_string());
                    }
                }
                Err(e) => notes.push(format!("whois: {e:#}")),
            }
        }

        let may_ask_iana = args.source == Source::Auto && !args.no_iana;
        // Without a bundled server and without IANA to ask, say so rather than
        // reporting an unknown with no reason attached.
        if !settled && status == Status::Unknown && !may_ask_iana && bundled.is_none() {
            notes.push(format!("no WHOIS server known for .{tld}"));
        }
        if !settled && status == Status::Unknown && may_ask_iana {
            match ctx.tld_info(&tld).await.and_then(|i| i.whois_host.clone()) {
                Some(host) => {
                    let already = bundled.as_ref().map(|s| s.endpoint.label());
                    if already.as_deref() != Some(host.as_str()) {
                        let server = whois::referred_server(host);
                        match whois::query(
                            &ctx.client,
                            &ctx.resolver,
                            &ctx.limiter,
                            &server,
                            &domain,
                            ctx.timeout,
                        )
                        .await
                        {
                            Ok(resp) => {
                                if resp.status != Status::Unknown {
                                    // The IANA-referred server answered: it wins.
                                    notes.clear();
                                    notes.push(format!("via iana referral {}", resp.server));
                                    status = resp.status;
                                    method = Method::Whois;
                                    whois_server = Some(resp.server.clone());
                                    if details.as_ref().is_none_or(Details::is_empty)
                                        && resp.status == Status::Taken
                                    {
                                        let parsed = whois::parse_details(&resp.raw);
                                        if !parsed.is_empty() {
                                            details = Some(parsed);
                                        }
                                    }
                                    if args.whois {
                                        whois_raw = Some(resp.raw.trim().to_string());
                                    }
                                } else if let Some(n) = resp.note {
                                    notes.push(format!("iana referral {}: {n}", resp.server));
                                }
                            }
                            Err(e) => notes.push(format!("iana referral: {e:#}")),
                        }
                    }
                }
                None if bundled.is_none() => {
                    notes.push(format!("no WHOIS server known for .{tld}"));
                }
                None => {}
            }
        }
    }

    // 3. Registrability. A brand or reserved TLD has no public registrations,
    //    so "no such domain" there is not an offer — reporting it as AVAILABLE
    //    would be exactly the dead end this tool exists to prevent. The reason
    //    is attached whatever the status, because it is worth knowing for a
    //    taken name too; it goes on last so the IANA-referral path above cannot
    //    clear it.
    if let Some(entry) = ctx.private.lookup(&tld) {
        notes.push(entry.reason(&tld));
        if status == Status::Available {
            status = Status::Private;
        }
    }

    // 4. DNS
    let dns = match &ctx.records {
        Some(r) => Some(dns::lookup(r, &domain).await),
        None => None,
    };

    // 5. Where to register — only meaningful for a name that is still free.
    let register = if args.show_where && status == Status::Available {
        let info = if args.no_iana {
            None
        } else {
            ctx.tld_info(&tld).await
        };
        Some(Register {
            registry: info.as_ref().and_then(|i| i.organisation.clone()),
            info_url: info.as_ref().and_then(|i| i.registration_url.clone()),
            eligibility: ctx.rules.lookup(&tld).cloned(),
            requirement: ctx.rules.requirement(&tld).cloned(),
            registrars: registration::listings(&ctx.prices, &tld, &domain),
        })
    } else {
        None
    };

    CheckResult {
        price: ctx.prices.lookup(&tld),
        domain,
        tld,
        status,
        method,
        registry: args.registry.then_some(registry_label).flatten(),
        whois_server: args.registry.then_some(whois_server).flatten(),
        note: (!notes.is_empty()).then(|| notes.join("; ")),
        details: args.details.then_some(details).flatten(),
        whois_raw,
        rdap_raw,
        dns,
        register,
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}

fn print_result(args: &Args, r: &CheckResult) {
    let (mark, color) = match r.status {
        Status::Available => ("+", Color::Green),
        Status::Taken => ("-", Color::Red),
        Status::Private => ("!", Color::Cyan),
        Status::Unknown => ("?", Color::Yellow),
    };

    // The price is the TLD's, so it is printed whatever the status: it is what
    // the name would cost, not a quote for this domain.
    let price = match &r.price {
        Some(p) => p.label(),
        None => "-".into(),
    };

    // Pad before painting: ANSI codes would otherwise count toward the width.
    println!(
        "{} {} {} {} {:<6} {:>6}ms{}",
        paint(mark, color),
        paint(&format!("{:<32}", r.domain), Color::Bold),
        paint(&format!("{:<10}", r.status.as_str()), color),
        paint(&format!("{price:>8}"), Color::Dim),
        r.method.as_str(),
        r.elapsed_ms,
        match (&r.note, r.status) {
            // PRIVATE without the reason attached is just a bare assertion.
            (Some(n), Status::Unknown | Status::Private) => format!("  {}", paint(n, Color::Dim)),
            _ => String::new(),
        }
    );

    if args.registry {
        if let Some(reg) = &r.registry {
            println!("    {} {}", paint("rdap:", Color::Dim), reg);
        }
        if let Some(ws) = &r.whois_server {
            println!("    {} {}", paint("whois:", Color::Dim), ws);
        }
    }

    if let Some(d) = &r.details {
        let field = |k: &str, v: &Option<String>| {
            if let Some(v) = v {
                println!("    {:<12} {v}", paint(k, Color::Dim));
            }
        };
        field("registrar", &d.registrar);
        field("registrant", &d.registrant);
        field("created", &d.created);
        field("updated", &d.updated);
        field("expires", &d.expires);
        if let Some(id) = &d.registrar_id {
            println!("    {:<12} {id}", paint("iana id", Color::Dim));
        }
        if !d.statuses.is_empty() {
            println!(
                "    {:<12} {}",
                paint("status", Color::Dim),
                d.statuses.join(", ")
            );
        }
        if !d.nameservers.is_empty() {
            println!(
                "    {:<12} {}",
                paint("nameservers", Color::Dim),
                d.nameservers.join(", ")
            );
        }
        if let Some(sec) = d.dnssec {
            println!(
                "    {:<12} {}",
                paint("dnssec", Color::Dim),
                if sec { "signed" } else { "unsigned" }
            );
        }
        field("abuse", &d.abuse_email);
    }

    if let Some(dns) = &r.dns {
        if dns.records.is_empty() {
            println!("    {:<12} no records", paint("dns", Color::Dim));
        } else {
            for (rtype, values) in &dns.records {
                let label = paint(&format!("{:<12}", rtype.to_ascii_lowercase()), Color::Dim);
                let joined = values.join(", ");
                if joined.len() <= 100 {
                    println!("    {label} {joined}");
                } else {
                    // Long record sets (TXT in particular) read better one per line.
                    for (i, v) in values.iter().enumerate() {
                        if i == 0 {
                            println!("    {label} {v}");
                        } else {
                            println!("    {:<12} {v}", "");
                        }
                    }
                }
            }
        }
    }

    if let Some(reg) = &r.register {
        if let Some(registry) = &reg.registry {
            println!("    {:<12} {registry}", paint("registry", Color::Dim));
        }
        if let Some(url) = &reg.info_url {
            println!("    {:<12} {url}", paint("registry url", Color::Dim));
        }

        // Before the shopping links: a name being free says nothing about
        // whether this registry would let you have it.
        if let Some(rule) = &reg.eligibility {
            println!(
                "    {:<12} {}",
                paint("eligibility", Color::Dim),
                paint(&rule.note, Color::Yellow)
            );
            println!("    {:<12} {}", "", rule.source);
        }

        // And what the TLD will expect of the name afterwards. Not a gate on
        // buying it — .app sells to anyone — so it is said separately.
        if let Some(rule) = &reg.requirement {
            println!(
                "    {:<12} {}",
                paint("requirement", Color::Dim),
                paint(&rule.note, Color::Yellow)
            );
            println!("    {:<12} {}", "", rule.source);
        }

        if reg.registrars.is_empty() {
            println!(
                "    {:<12} {}",
                paint("register at", Color::Dim),
                paint(
                    &format!("no registrar in the price table sells .{}", r.tld),
                    Color::Dim
                )
            );
        } else {
            // The registrar names line up so the prices beside them compare.
            let width = reg
                .registrars
                .iter()
                .map(|l| l.registrar.len())
                .max()
                .unwrap_or(0);
            for (i, l) in reg.registrars.iter().enumerate() {
                let row = format!(
                    "{:<width$} {:>9}  {}",
                    l.registrar,
                    l.price_label(),
                    l.search.as_deref().unwrap_or("")
                );
                let row = row.trim_end();
                if i == 0 {
                    println!("    {:<12} {row}", paint("register at", Color::Dim));
                } else {
                    println!("    {:<12} {row}", "");
                }
            }
        }
    }

    if let Some(raw) = &r.whois_raw {
        println!("    {}", paint("--- whois ---", Color::Dim));
        for line in raw.lines() {
            println!("    {line}");
        }
        println!("    {}", paint("-------------", Color::Dim));
    }

    if let Some(raw) = &r.rdap_raw {
        println!("    {}", paint("--- rdap ---", Color::Dim));
        println!(
            "{}",
            serde_json::to_string_pretty(raw)
                .unwrap_or_default()
                .lines()
                .map(|l| format!("    {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        println!("    {}", paint("------------", Color::Dim));
    }
}

fn save(args: &Args, results: &[CheckResult]) -> Result<Vec<(PathBuf, usize)>> {
    use std::io::Write;

    let dir = args.out_dir.clone().unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;

    // --json asks for JSON everywhere, saved files included.
    let ext = if args.json { "json" } else { "txt" };
    let groups = [
        (Status::Available, "available"),
        (Status::Taken, "unavailable"),
        (Status::Private, "private"),
        (Status::Unknown, "unknown"),
    ];

    let mut written = Vec::new();
    for (status, stem) in groups {
        let matching: Vec<&CheckResult> = results.iter().filter(|r| r.status == status).collect();

        // Only create the unknown and private files when there is something
        // to put in them; the other two are always written.
        if matches!(status, Status::Unknown | Status::Private) && matching.is_empty() {
            continue;
        }

        let path = dir.join(format!("{stem}.{ext}"));

        if args.json {
            // Two JSON arrays back to back are not a JSON file, so appending
            // means merging with whatever the last run left behind.
            let mut entries = if args.append {
                saved_entries(&path)?
            } else {
                Vec::new()
            };
            for r in &matching {
                entries.push(serde_json::to_value(r)?);
            }
            let body = serde_json::to_string_pretty(&entries)?;
            std::fs::write(&path, format!("{body}\n"))
                .with_context(|| format!("writing {}", path.display()))?;
        } else {
            let mut file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(args.append)
                .truncate(!args.append)
                .open(&path)
                .with_context(|| format!("writing {}", path.display()))?;
            for r in &matching {
                writeln!(file, "{}", r.domain)?;
            }
        }

        written.push((path, matching.len()));
    }

    Ok(written)
}

/// The entries already in a saved JSON file, so `--append` extends that array.
/// A missing file is an empty one; anything else there is left alone rather
/// than overwritten.
fn saved_entries(path: &Path) -> Result<Vec<serde_json::Value>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(&text).with_context(|| {
        format!(
            "{} is not a JSON array — --append cannot extend it",
            path.display()
        )
    })
}

fn summarize(results: &[CheckResult], elapsed: Duration, written: &[(PathBuf, usize)]) {
    let available = results
        .iter()
        .filter(|r| r.status == Status::Available)
        .count();
    let taken = results.iter().filter(|r| r.status == Status::Taken).count();
    let private = results
        .iter()
        .filter(|r| r.status == Status::Private)
        .count();
    let unknown = results.len() - available - taken - private;

    // Most runs have no private TLDs in them; a permanent `0 private` column
    // would be noise for everyone else.
    let private_part = if private == 0 {
        String::new()
    } else {
        format!("  {} private", paint(&private.to_string(), Color::Cyan))
    };

    println!(
        "\n{} {} available  {} taken{private_part}  {} unknown   ({} checked in {:.1}s)",
        paint("summary:", Color::Bold),
        paint(&available.to_string(), Color::Green),
        paint(&taken.to_string(), Color::Red),
        paint(&unknown.to_string(), Color::Yellow),
        results.len(),
        elapsed.as_secs_f64()
    );

    for (path, count) in written {
        println!(
            "{} {} ({count} entr{})",
            paint("wrote:", Color::Dim),
            path.display(),
            if *count == 1 { "y" } else { "ies" }
        );
    }

    // Rate-limited lookups are worth re-running; dead servers are not.
    let throttled = results
        .iter()
        .filter(|r| {
            r.status == Status::Unknown
                && r.note
                    .as_deref()
                    .is_some_and(|n| n.contains("rate-limited") || n.contains("HTTP 403"))
        })
        .count();
    if throttled > 0 {
        println!(
            "{} {throttled} lookup{} hit registry rate limits — re-run those TLDs later",
            paint("note:", Color::Yellow),
            if throttled == 1 { "" } else { "s" }
        );
    }
}

#[derive(Clone, Copy)]
enum Color {
    Green,
    Red,
    Yellow,
    Cyan,
    Dim,
    Bold,
}

/// The Windows console prints escape sequences literally (`←[31m`) until
/// virtual-terminal processing is switched on for the handle. Turn it on for
/// stdout and stderr; if stdout refuses, the console is too old for ANSI and
/// the caller drops colour entirely.
#[cfg(windows)]
fn enable_ansi() -> bool {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, CONSOLE_MODE,
        ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_ERROR_HANDLE, STD_HANDLE, STD_OUTPUT_HANDLE,
    };

    fn enable(which: STD_HANDLE) -> bool {
        unsafe {
            let handle = GetStdHandle(which);
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                return false;
            }
            let mut mode: CONSOLE_MODE = 0;
            if GetConsoleMode(handle, &mut mode) == 0 {
                // Not a console — a pipe or file, which never needed the flag.
                return true;
            }
            if mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0 {
                return true;
            }
            SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
        }
    }

    let stdout_ok = enable(STD_OUTPUT_HANDLE);
    // `error:` is painted on stderr, so that handle needs the flag as well.
    enable(STD_ERROR_HANDLE);
    stdout_ok
}

#[cfg(not(windows))]
fn enable_ansi() -> bool {
    true
}

fn paint(text: &str, color: Color) -> String {
    if !COLOR.load(Ordering::Relaxed) {
        return text.to_string();
    }
    let code = match color {
        Color::Green => "32",
        Color::Red => "31",
        Color::Yellow => "33",
        Color::Cyan => "36",
        Color::Dim => "2",
        Color::Bold => "1",
    };
    format!("\x1b[{code}m{text}\x1b[0m")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn splits_comma_lists_and_arguments() {
        let names = expand_names(&args(&["apple,orange, bangla", "english"])).unwrap();
        assert_eq!(names, ["apple", "orange", "bangla", "english"]);
    }

    #[test]
    fn normalises_and_deduplicates() {
        let names = expand_names(&args(&["Apple.", ".apple", "APPLE", "orange"])).unwrap();
        assert_eq!(names, ["apple", "orange"]);
    }

    #[test]
    fn parses_tld_length_specs() {
        assert_eq!(LenRange::parse("2").unwrap(), LenRange { min: 2, max: 2 });
        assert_eq!(LenRange::parse("2-3").unwrap(), LenRange { min: 2, max: 3 });
        assert_eq!(LenRange::parse("-3").unwrap(), LenRange { min: 1, max: 3 });
        assert_eq!(LenRange::parse("4-").unwrap().min, 4);
        assert!(LenRange::parse("nope").is_err());
        assert!(LenRange::parse("0").is_err());
        assert!(LenRange::parse("5-2").is_err());
    }

    #[test]
    fn tld_length_is_measured_on_the_last_label() {
        let two = LenRange::parse("2").unwrap();
        assert!(two.allows("io"));
        assert!(two.allows("co.uk"), "the TLD of co.uk is uk");
        assert!(!two.allows("com"));
        assert!(!two.allows("xn--p1ai"));

        let short = LenRange::parse("-3").unwrap();
        assert!(short.allows("com") && short.allows("de"));
        assert!(!short.allows("info"));
    }

    #[test]
    fn cctld_is_shorthand_for_two_letters() {
        let a = Args::try_parse_from(["ds", "apple", "--cctld"]).unwrap();
        assert!(a.cctld && a.tld_len.is_none());

        let two = LenRange::parse("2").unwrap();
        // Every two-letter TLD is a country code, sub-zones included.
        for tld in ["de", "io", "co.uk", "com.au"] {
            assert!(two.allows(tld), "{tld}");
        }
        for tld in ["com", "app", "xn--p1ai"] {
            assert!(!two.allows(tld), "{tld}");
        }
    }

    #[test]
    fn accepts_open_ended_ranges_as_arguments() {
        // `-3` must not be parsed as a flag.
        let a = Args::try_parse_from(["ds", "apple", "--tld-len", "-3"]).unwrap();
        assert_eq!(a.tld_len.as_deref(), Some("-3"));
    }

    #[test]
    fn parses_the_source_option() {
        assert_eq!(
            Args::try_parse_from(["ds", "apple"]).unwrap().source,
            Source::Auto
        );
        assert_eq!(
            Args::try_parse_from(["ds", "apple", "--source", "whois"])
                .unwrap()
                .source,
            Source::Whois
        );
        assert_eq!(
            Args::try_parse_from(["ds", "apple", "--source", "rdap"])
                .unwrap()
                .source,
            Source::Rdap
        );
        assert!(Args::try_parse_from(["ds", "apple", "--source", "nope"]).is_err());
    }

    #[test]
    fn registrar_searches_carry_the_full_domain() {
        let prices = Prices::load().unwrap();
        let links = registration::listings(&prices, "io", "apple.io");

        assert!(!links.is_empty(), ".io is priced in the bundled table");
        assert!(links
            .iter()
            .filter_map(|l| l.search.as_deref())
            .all(|s| s.ends_with("apple.io")));
    }

    #[test]
    fn a_restricted_tld_says_so_before_it_says_where() {
        let rules = Rules::load().unwrap();
        assert!(rules.lookup("eu").is_some());
        // The two halves are independent: a TLD can be restricted and still
        // widely sold, which is exactly the case worth warning about.
        assert!(!registration::listings(&Prices::load().unwrap(), "eu", "apple.eu").is_empty());
    }

    #[test]
    fn an_https_only_tld_is_still_sold_to_anyone() {
        let rules = Rules::load().unwrap();
        // .app is the case the two maps exist to tell apart: no gate on the
        // buyer, but a condition on the name, and it is on sale everywhere.
        assert!(rules.requirement("app").is_some());
        assert!(rules.lookup("app").is_none());
        assert!(!registration::listings(&Prices::load().unwrap(), "app", "apple.app").is_empty());
    }

    #[test]
    fn rejects_an_empty_list() {
        assert!(expand_names(&args(&[" ", ","])).is_err());
    }

    /// A bootstrap holding one open gTLD and two Specification 13 brands.
    fn bootstrap_with_brands() -> Bootstrap {
        // Unique per call: these tests run in parallel and would otherwise
        // delete each other's copy of the file.
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("ds-private-rdap-{n}.json"));
        std::fs::write(
            &path,
            r#"{"services":[[["com","aws","google"],["https://rdap.example/"]]]}"#,
        )
        .unwrap();
        let b = Bootstrap::from_file(&path).unwrap();
        std::fs::remove_file(&path).ok();
        b
    }

    fn selected(argv: &[&str]) -> Result<Vec<String>> {
        let args = Args::try_parse_from(argv)?;
        let targets = build_targets(
            &args,
            &Registry::default(),
            &bootstrap_with_brands(),
            &PrivateTlds::load().unwrap(),
        )?;
        Ok(targets.into_iter().map(|t| t.tld).collect())
    }

    #[test]
    fn a_keyword_list_drops_private_tlds_by_default() {
        assert_eq!(
            selected(&["ds", "apple", "--tld", "rdap"]).unwrap(),
            ["com"]
        );
    }

    #[test]
    fn private_include_and_only_select_the_brands() {
        let mut all = selected(&["ds", "apple", "--tld", "rdap", "--private", "include"]).unwrap();
        all.sort();
        assert_eq!(all, ["aws", "com", "google"]);

        let mut only = selected(&["ds", "apple", "--tld", "rdap", "--private", "only"]).unwrap();
        only.sort();
        assert_eq!(only, ["aws", "google"]);
    }

    #[test]
    fn naming_a_private_tld_still_checks_it() {
        // The default exclusion prunes lists `ds` chose; it must never silently
        // discard a TLD the user typed out.
        let mut named = selected(&["ds", "apple", "--tld", "aws,com"]).unwrap();
        named.sort();
        assert_eq!(named, ["aws", "com"]);

        // Asking for the filter by hand does apply it to a hand-written list.
        assert_eq!(
            selected(&["ds", "apple", "--tld", "aws,com", "--private", "exclude"]).unwrap(),
            ["com"]
        );
    }

    #[test]
    fn an_empty_selection_is_an_error_not_an_empty_run() {
        assert!(selected(&["ds", "apple", "--tld", "aws", "--private", "exclude"]).is_err());
        assert!(selected(&["ds", "apple", "--tld", "com", "--private", "only"]).is_err());
    }

    #[test]
    fn private_is_its_own_status_in_json() {
        assert_eq!(Status::Private.as_str(), "PRIVATE");
        assert_eq!(
            serde_json::to_string(&Status::Private).unwrap(),
            "\"private\""
        );
    }

    #[test]
    fn reads_names_from_a_file() {
        let path = std::env::temp_dir().join("ds-names-test.txt");
        std::fs::write(&path, "apple\norange, bangla  # two here\n\nenglish\n").unwrap();
        let names = expand_names(&args(&[&format!("@{}", path.display())])).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(names, ["apple", "orange", "bangla", "english"]);
    }

    /// `--serve` makes `names` conditionally required, which is the risky
    /// part of the flag, so it is pinned here.
    #[cfg(feature = "serve")]
    #[test]
    fn the_serve_flag_leaves_name_parsing_alone() {
        let a = Args::try_parse_from(["ds", "apple", "--tld", "com"]).unwrap();
        assert!(!a.serve);
        assert_eq!(a.names, ["apple"]);
        assert_eq!(a.tld.as_deref(), Some("com"));

        // `serve` is a name like any other: no subcommand shadows it.
        let many = Args::try_parse_from(["ds", "serve", "serverless", "--tld", "com"]).unwrap();
        assert!(!many.serve);
        assert_eq!(many.names, ["serve", "serverless"]);

        // `ds` with nothing at all still asks for a name rather than starting
        // a server or doing nothing.
        assert!(Args::try_parse_from(["ds"]).is_err());

        // ... but `--serve` alone is enough, and takes the shared options.
        let s =
            Args::try_parse_from(["ds", "--serve", "--port", "9000", "--per-host", "2"]).unwrap();
        assert!(s.serve);
        assert!(s.names.is_empty());
        assert_eq!(s.serve_opts.port, 9000);
        assert_eq!(s.per_host, 2);

        // A server option without --serve is a mistake worth reporting.
        assert!(Args::try_parse_from(["ds", "apple", "--port", "9000"]).is_err());
    }
}
