//! `dc` — a simple domain availability checker.
//!
//! RDAP first (IANA bootstrap), WHOIS fallback for TLDs with no RDAP service,
//! using the server/needle table in the bundled `dist.whois.json`.

mod bootstrap;
mod dns;
mod limit;
mod model;
mod rdap;
mod tlds;
mod whois;

use std::collections::BTreeSet;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use clap::Parser;
use futures::stream::StreamExt;
use hickory_resolver::TokioAsyncResolver;

use bootstrap::Bootstrap;
use limit::HostLimiter;
use model::{CheckResult, Details, Method, Register, Status};
use tlds::Registry;
use whois::TldInfo;

const USER_AGENT: &str = concat!("dc/", env!("CARGO_PKG_VERSION"), " (domain-checker)");

#[derive(Parser, Debug)]
#[command(
    name = "dc",
    version,
    about = "Check domain availability over RDAP with a WHOIS fallback",
    long_about = "Check domain availability over RDAP with a WHOIS fallback.\n\n\
                  Examples:\n  \
                  dc apple --tld all\n  \
                  dc apple --tld com,net --details\n  \
                  dc apple,orange,bangla,english --tld com,net\n  \
                  dc @names.txt --tld popular --available-only\n  \
                  dc apple --tld com,io --where\n  \
                  dc apple google --tld popular --whois --dns-records\n  \
                  dc apple.com --details --registry"
)]
struct Args {
    /// Name(s) to check: `apple`, `apple,orange,bangla`, `@names.txt`, or
    /// several arguments. A full domain (`apple.com`) is checked as-is when
    /// --tld is omitted.
    #[arg(required = true)]
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

    /// For available domains, show the registry and where to register them.
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

    /// Only print available domains (both files are still written).
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

    /// Directory for available.txt / unavailable.txt.
    #[arg(short, long, default_value = ".")]
    out_dir: PathBuf,

    /// Do not write any output files.
    #[arg(long = "no-save")]
    no_save: bool,

    /// Append to the output files instead of overwriting them.
    #[arg(long)]
    append: bool,

    /// Skip RDAP and use WHOIS only.
    #[arg(long = "whois-only")]
    whois_only: bool,

    /// Skip the WHOIS fallback (RDAP only).
    #[arg(long = "no-whois")]
    no_whois: bool,

    /// Re-download the IANA RDAP bootstrap file.
    #[arg(long = "refresh")]
    refresh: bool,

    /// Suppress progress output; only the summary is printed.
    #[arg(short, long)]
    quiet: bool,

    /// Disable coloured output.
    #[arg(long = "no-color")]
    no_color: bool,
}

static COLOR: AtomicBool = AtomicBool::new(false);

struct Ctx {
    args: Args,
    client: reqwest::Client,
    limiter: HostLimiter,
    bootstrap: Bootstrap,
    registry: Registry,
    resolver: Option<TokioAsyncResolver>,
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
        let info = whois::iana_tld_info(&self.limiter, &apex, self.timeout)
            .await
            .ok()
            .map(Arc::new);
        self.tld_info.lock().await.insert(apex, info.clone());
        info
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("{} {e:#}", paint("error:", Color::Red));
        std::process::exit(2);
    }
}

async fn run() -> Result<()> {
    let mut args = Args::parse();
    if args.all_info {
        args.details = true;
        args.registry = true;
        args.whois = true;
        args.dns_records = true;
        args.show_where = true;
    }
    if args.concurrency == 0 {
        args.concurrency = 1;
    }
    COLOR.store(
        !args.no_color && std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal(),
        Ordering::Relaxed,
    );

    let timeout = Duration::from_secs(args.timeout.max(1));
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(timeout)
        .connect_timeout(timeout)
        .build()
        .context("building HTTP client")?;

    let registry = Registry::load().context("loading dist.whois.json")?;

    let bootstrap = if args.whois_only {
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
    if bootstrap.is_empty() && args.no_whois {
        bail!("no RDAP bootstrap data and --no-whois was given: nothing to query with");
    }

    let resolver = args.dns_records.then(|| dns::resolver(timeout));

    let targets = build_targets(&args, &registry, &bootstrap)?;
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
        args,
        client,
        bootstrap,
        registry,
        resolver,
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
            print_result(&ctx.args, &result);
        }
        results.push((i, result));
    }

    results.sort_by_key(|(i, _)| *i);
    let results: Vec<CheckResult> = results.into_iter().map(|(_, r)| r).collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    }

    let written = if ctx.args.no_save {
        Vec::new()
    } else {
        save(&ctx.args, &results)?
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

fn build_targets(args: &Args, registry: &Registry, bootstrap: &Bootstrap) -> Result<Vec<Target>> {
    let tld_list: Option<Vec<String>> = match &args.tld {
        Some(spec) => Some(resolve_tlds(spec, registry, bootstrap)?),
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

fn resolve_tlds(spec: &str, registry: &Registry, bootstrap: &Bootstrap) -> Result<Vec<String>> {
    let spec = spec.trim();

    if let Some(path) = spec.strip_prefix('@') {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading TLD list from {path}"))?;
        return Ok(dedup(
            text.lines()
                .map(|l| l.split('#').next().unwrap_or("").to_string())
                .flat_map(|l| {
                    l.split(',')
                        .map(tlds::normalize_tld)
                        .filter(|t| !t.is_empty())
                        .collect::<Vec<_>>()
                })
                .collect(),
        ));
    }

    match spec.to_ascii_lowercase().as_str() {
        "all" => {
            let mut set = registry.all_tlds();
            set.extend(bootstrap.all_tlds());
            Ok(set.into_iter().collect())
        }
        "rdap" => Ok(bootstrap.all_tlds().into_iter().collect()),
        "popular" => Ok(tlds::POPULAR.iter().map(|t| t.to_string()).collect()),
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
            Ok(list)
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
    let args = &ctx.args;

    let mut status = Status::Unknown;
    let mut method = Method::None;
    let mut registry_label = None;
    let mut whois_server = None;
    let mut notes: Vec<String> = Vec::new();
    let mut details: Option<Details> = None;
    let mut rdap_raw = None;
    let mut whois_raw = None;

    // 1. RDAP
    if !args.whois_only {
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
    let need_whois = !args.no_whois && (status == Status::Unknown || args.whois);
    if need_whois {
        let bundled = ctx.registry.lookup(&tld).cloned();
        let mut settled = false;

        if let Some(server) = &bundled {
            match whois::query(&ctx.client, &ctx.limiter, server, &domain, ctx.timeout).await {
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

        if !settled && status == Status::Unknown {
            match ctx.tld_info(&tld).await.and_then(|i| i.whois_host.clone()) {
                Some(host) => {
                    let already = bundled.as_ref().map(|s| s.endpoint.label());
                    if already.as_deref() != Some(host.as_str()) {
                        let server = whois::referred_server(host);
                        match whois::query(&ctx.client, &ctx.limiter, &server, &domain, ctx.timeout)
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

    // 3. DNS
    let dns = match &ctx.resolver {
        Some(r) => Some(dns::lookup(r, &domain).await),
        None => None,
    };

    // 4. Where to register — only meaningful for a name that is still free.
    let register = if args.show_where && status == Status::Available {
        let info = ctx.tld_info(&tld).await;
        Some(Register {
            registry: info.as_ref().and_then(|i| i.organisation.clone()),
            info_url: info.as_ref().and_then(|i| i.registration_url.clone()),
            search: registrar_searches(&domain),
        })
    } else {
        None
    };

    CheckResult {
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

/// Registrar searches with the domain filled in. Deliberately not a claim that
/// a given registrar sells the TLD — that is for the registrar's page to say.
fn registrar_searches(domain: &str) -> Vec<String> {
    [
        "https://porkbun.com/checkout/search?q=",
        "https://www.namecheap.com/domains/registration/results/?domain=",
        "https://www.dynadot.com/domain/search?domain=",
        "https://www.namesilo.com/domain/search-domains?query=",
    ]
    .iter()
    .map(|prefix| format!("{prefix}{domain}"))
    .collect()
}

fn print_result(args: &Args, r: &CheckResult) {
    let (mark, color) = match r.status {
        Status::Available => ("+", Color::Green),
        Status::Taken => ("-", Color::Red),
        Status::Unknown => ("?", Color::Yellow),
    };

    // Pad before painting: ANSI codes would otherwise count toward the width.
    println!(
        "{} {} {} {:<6} {:>6}ms{}",
        paint(mark, color),
        paint(&format!("{:<32}", r.domain), Color::Bold),
        paint(&format!("{:<10}", r.status.as_str()), color),
        r.method.as_str(),
        r.elapsed_ms,
        match (&r.note, r.status) {
            (Some(n), Status::Unknown) => format!("  {}", paint(n, Color::Dim)),
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
        for (i, url) in reg.search.iter().enumerate() {
            if i == 0 {
                println!("    {:<12} {url}", paint("register at", Color::Dim));
            } else {
                println!("    {:<12} {url}", "");
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

    std::fs::create_dir_all(&args.out_dir)
        .with_context(|| format!("creating {}", args.out_dir.display()))?;

    let groups = [
        (Status::Available, "available.txt"),
        (Status::Taken, "unavailable.txt"),
        (Status::Unknown, "unknown.txt"),
    ];

    let mut written = Vec::new();
    for (status, filename) in groups {
        let lines: Vec<&str> = results
            .iter()
            .filter(|r| r.status == status)
            .map(|r| r.domain.as_str())
            .collect();

        // Only create unknown.txt when there is something to put in it.
        if status == Status::Unknown && lines.is_empty() {
            continue;
        }

        let path = args.out_dir.join(filename);
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(args.append)
            .truncate(!args.append)
            .open(&path)
            .with_context(|| format!("writing {}", path.display()))?;
        for line in &lines {
            writeln!(file, "{line}")?;
        }
        written.push((path, lines.len()));
    }

    Ok(written)
}

fn summarize(results: &[CheckResult], elapsed: Duration, written: &[(PathBuf, usize)]) {
    let available = results
        .iter()
        .filter(|r| r.status == Status::Available)
        .count();
    let taken = results.iter().filter(|r| r.status == Status::Taken).count();
    let unknown = results.len() - available - taken;

    println!(
        "\n{} {} available  {} taken  {} unknown   ({} checked in {:.1}s)",
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
    Dim,
    Bold,
}

fn paint(text: &str, color: Color) -> String {
    if !COLOR.load(Ordering::Relaxed) {
        return text.to_string();
    }
    let code = match color {
        Color::Green => "32",
        Color::Red => "31",
        Color::Yellow => "33",
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
    fn builds_registrar_searches_for_the_full_domain() {
        let links = registrar_searches("apple.io");
        assert_eq!(links.len(), 4);
        assert!(links.iter().all(|l| l.ends_with("apple.io")));
        assert!(links[0].starts_with("https://porkbun.com/"));
    }

    #[test]
    fn rejects_an_empty_list() {
        assert!(expand_names(&args(&[" ", ","])).is_err());
    }

    #[test]
    fn reads_names_from_a_file() {
        let path = std::env::temp_dir().join("dc-names-test.txt");
        std::fs::write(&path, "apple\norange, bangla  # two here\n\nenglish\n").unwrap();
        let names = expand_names(&args(&[&format!("@{}", path.display())])).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(names, ["apple", "orange", "bangla", "english"]);
    }
}
