//! `ds serve` — the same checks the CLI runs, over HTTP.
//!
//! Behind the `serve` cargo feature, so a default build (and every shipped
//! package) is byte-for-byte the CLI it was before.
//!
//! The thing to keep in mind here is that a `ds` server is an open proxy onto
//! other people's registries. `src/limit.rs` exists because a single `--tld
//! all` run can get a registry annoyed; a server exposes that same machinery
//! to anyone who can reach the port. So: loopback by default, a cap on how
//! much one request may ask for, one shared `HostLimiter` for the whole
//! process rather than one per request, a per-client rate limit, and a cache
//! so a page that polls the same name does not put a registry through it
//! again.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use bytes::Bytes;
use futures::stream::StreamExt;
use http_body_util::Full;
use hyper::header::{ALLOW, CACHE_CONTROL, CONTENT_TYPE, RETRY_AFTER};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioIo, TokioTimer};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Semaphore};

use crate::bootstrap::Bootstrap;
use crate::limit::HostLimiter;
use crate::model::{CheckResult, Status};
use crate::pricing::Prices;
use crate::private::PrivateTlds;
use crate::registration::Rules;
use crate::tlds::Registry;
use crate::{check, dns, resolve_tlds, Ctx, Lookup, Selection, Source};

/// Entries kept before the cache is swept. Small on purpose: this is a cache
/// in front of a network, not a database.
const CACHE_MAX: usize = 4096;

/// An UNKNOWN is a failed lookup, not a fact about the domain, so it is held
/// for a fraction of the normal TTL: long enough to stop a client's retry loop
/// from hammering a registry that is already refusing us, short enough that a
/// blip does not stick to a name for the rest of the hour.
const UNKNOWN_TTL: Duration = Duration::from_secs(30);

/// Clients tracked by the rate limiter before stale windows are dropped.
const CLIENTS_MAX: usize = 8192;

/// How long a client has to finish sending its request line and headers.
const HEADER_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(clap::Args, Debug)]
pub struct ServeArgs {
    /// Address to bind. Loopback by default and deliberately: a reachable
    /// `ds` server queries registries on behalf of whoever asks it to.
    #[arg(long, default_value = "127.0.0.1", value_name = "ADDR")]
    host: String,

    /// Port to listen on.
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    /// Most domains one request may ask about (names × TLDs). The default
    /// leaves room for `tld=popular` and rules out `tld=all`.
    #[arg(long = "max-lookups", default_value_t = 50, value_name = "N")]
    max_lookups: usize,

    /// Requests per minute per client address; 0 turns the limit off.
    #[arg(long = "rate-limit", default_value_t = 60, value_name = "N")]
    rate_limit: u32,

    /// How long an answer is reused before the registry is asked again;
    /// 0 disables the cache.
    #[arg(long = "cache-ttl", default_value_t = 300, value_name = "SECONDS")]
    cache_ttl: u64,

    /// Lookups in flight across the whole server, however many clients are
    /// connected.
    #[arg(short, long, default_value_t = 20)]
    concurrency: usize,

    /// Parallel lookups against any single registry server.
    #[arg(long = "per-host", default_value_t = 4)]
    per_host: usize,

    /// Per-request timeout in seconds.
    #[arg(long, default_value_t = 10)]
    timeout: u64,

    /// Send `Access-Control-Allow-Origin` with this value, so a browser page
    /// may call the API. An origin (`https://example.com`) or `*`.
    #[arg(long, value_name = "ORIGIN")]
    cors: Option<String>,

    /// Where to look: auto (RDAP, then the bundled WHOIS table, then an IANA
    /// referral), rdap (RDAP only), or whois (bundled whois.json only).
    #[arg(long, value_enum, default_value_t = Source::Auto)]
    source: Source,

    /// Never query whois.iana.org.
    #[arg(long = "no-iana")]
    no_iana: bool,

    /// Include registration details (registrar, dates, status, nameservers)
    /// in the response. Off by default: some registries answer with the
    /// registrant's name and address.
    #[arg(long)]
    details: bool,

    /// Include which registry answered (RDAP endpoint / WHOIS server).
    #[arg(long)]
    registry: bool,

    /// For available domains, include the registry and where to register
    /// them. Costs one extra IANA lookup per TLD, cached for the process.
    #[arg(long = "where")]
    show_where: bool,
}

/// Everything a request needs. One per process, shared by every connection —
/// in particular the `HostLimiter` inside `Ctx`, so pacing and the circuit
/// breaker are counted across clients rather than reset by each one.
struct State {
    ctx: Arc<Ctx>,
    cache: Cache,
    rate: RateLimiter,
    slots: Semaphore,
    max_lookups: usize,
    cors: Option<String>,
}

pub async fn run(args: ServeArgs) -> Result<()> {
    let timeout = Duration::from_secs(args.timeout.max(1));
    let resolver = dns::connect_resolver(timeout);
    let client = reqwest::Client::builder()
        .user_agent(crate::USER_AGENT)
        .timeout(timeout)
        .connect_timeout(timeout)
        .dns_resolver(dns::Dns::new(&resolver))
        .build()
        .context("building HTTP client")?;

    // The server uses the bundled tables only. Picking up ./whois.json the way
    // the CLI does would make the answers depend on the working directory a
    // service happened to be started from.
    let registry = Registry::load().context("loading the bundled whois.json")?;
    let prices = Prices::load().context("loading the bundled pricing.json")?;
    let private = PrivateTlds::load().context("loading the bundled private-tlds.json")?;
    let bootstrap = if args.source == Source::Whois {
        Bootstrap::default()
    } else {
        match Bootstrap::load(&client, false).await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("warning: RDAP bootstrap unavailable ({e:#}); falling back to WHOIS");
                Bootstrap::default()
            }
        }
    };

    let ctx = Arc::new(Ctx {
        opts: Lookup {
            source: args.source,
            details: args.details,
            registry: args.registry,
            show_where: args.show_where,
            no_iana: args.no_iana,
            // Raw WHOIS and raw RDAP are never served: they are large, they are
            // the registry's text to publish rather than ours to mirror, and
            // WHOIS records carry contact data the parsed details leave out.
            whois: false,
            raw: false,
            json: false,
        },
        client,
        limiter: HostLimiter::new(args.per_host),
        bootstrap,
        registry,
        prices,
        private,
        rules: Rules::load().context("loading the bundled eligibility.json")?,
        resolver,
        records: None,
        timeout,
        tld_info: Mutex::new(HashMap::new()),
    });

    let state = Arc::new(State {
        ctx,
        cache: Cache::new(Duration::from_secs(args.cache_ttl)),
        rate: RateLimiter::new(args.rate_limit),
        slots: Semaphore::new(args.concurrency.max(1)),
        max_lookups: args.max_lookups.max(1),
        cors: args.cors,
    });

    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .with_context(|| format!("{}:{} is not an address to bind", args.host, args.port))?;
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;

    println!("ds serve listening on http://{addr}");
    println!("  GET /v1/check?name=apple&tld=com,net");
    println!("  GET /healthz");
    println!(
        "  limits: {} lookups per request, {} concurrent, {} per host, {}",
        state.max_lookups,
        args.concurrency.max(1),
        args.per_host,
        match args.rate_limit {
            0 => "no per-client rate limit".to_string(),
            n => format!("{n} requests/min per client"),
        }
    );
    if !addr.ip().is_loopback() {
        eprintln!(
            "warning: {} is reachable from other machines. Every request this \
             server accepts becomes a query against someone else's registry, \
             under this host's address — put it behind an authenticating proxy \
             unless you mean to run it open.",
            addr.ip()
        );
    }

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(pair) => pair,
            // A single failed accept (a client that hung up, an exhausted fd
            // table) is not a reason to take the server down.
            Err(e) => {
                eprintln!("warning: accept failed: {e}");
                tokio::time::sleep(Duration::from_millis(50)).await;
                continue;
            }
        };

        let state = Arc::clone(&state);
        tokio::spawn(async move {
            let service = service_fn(move |req: Request<hyper::body::Incoming>| {
                let state = Arc::clone(&state);
                async move { Ok::<_, std::convert::Infallible>(handle(&state, peer.ip(), req).await) }
            });

            let conn = hyper::server::conn::http1::Builder::new()
                .timer(TokioTimer::new())
                // Without this a client can hold a connection open forever by
                // never finishing its headers.
                .header_read_timeout(HEADER_TIMEOUT)
                .serve_connection(TokioIo::new(stream), service);

            if let Err(e) = conn.await {
                // Clients disconnect mid-request all the time; it is not worth
                // more than a line on stderr.
                eprintln!("warning: connection from {peer}: {e}");
            }
        });
    }
}

// --- routing ---------------------------------------------------------------

async fn handle<B>(state: &State, peer: IpAddr, req: Request<B>) -> Response<Full<Bytes>> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let query = req.uri().query().unwrap_or("").to_string();

    let response = match (&method, path.as_str()) {
        // A GET with no custom headers is not preflighted, but a client that
        // sends one anyway should get an answer rather than a 405.
        (&Method::OPTIONS, _) if state.cors.is_some() => Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header("access-control-allow-methods", "GET, OPTIONS")
            .body(Full::new(Bytes::new()))
            .expect("static response"),
        (&Method::GET, "/healthz") => json(
            StatusCode::OK,
            &serde_json::json!({"status": "ok", "version": env!("CARGO_PKG_VERSION")}),
            "no-store",
        ),
        (&Method::GET, "/v1/check") => check_endpoint(state, peer, &query).await,
        (&Method::GET, _) => error(StatusCode::NOT_FOUND, "no such endpoint; try /v1/check"),
        _ => {
            let mut r = error(StatusCode::METHOD_NOT_ALLOWED, "only GET is served");
            r.headers_mut()
                .insert(ALLOW, "GET".parse().expect("static header"));
            r
        }
    };

    with_cors(response, state)
}

async fn check_endpoint(state: &State, peer: IpAddr, query: &str) -> Response<Full<Bytes>> {
    if let Err(retry_after) = state.rate.admit(peer).await {
        let mut r = error(
            StatusCode::TOO_MANY_REQUESTS,
            "rate limit exceeded for this client",
        );
        r.headers_mut().insert(
            RETRY_AFTER,
            retry_after
                .to_string()
                .parse()
                .expect("a number of seconds"),
        );
        return r;
    }

    let targets = match plan(state, query) {
        Ok(t) => t,
        Err(e) => return error(StatusCode::BAD_REQUEST, &e),
    };

    let (results, all_cached) = lookups(state, targets).await;

    // The body is exactly what `ds --json` prints, so a caller can move
    // between the CLI and the API without a second shape to parse. An
    // inconclusive lookup is "unknown" here just as it is there: the server
    // never turns one into "available".
    let body = match serde_json::to_value(&results) {
        Ok(v) => v,
        Err(e) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("serialising results: {e}"),
            )
        }
    };

    let cache_control = match state.cache.ttl.as_secs() {
        0 => "no-store".to_string(),
        // An unknown in the set is a lookup that failed, so the answer is not
        // one an intermediary should hold on to.
        _ if results.iter().any(|r| r.status == Status::Unknown) => "no-store".to_string(),
        secs => format!("public, max-age={secs}"),
    };

    let mut response = json(StatusCode::OK, &body, &cache_control);
    response.headers_mut().insert(
        "x-cache",
        if all_cached { "HIT" } else { "MISS" }
            .parse()
            .expect("static header"),
    );
    response
}

/// Run the lookups, answering from the cache where it can. The bool is true
/// when every result came from the cache.
async fn lookups(state: &State, targets: Vec<Target>) -> (Vec<CheckResult>, bool) {
    let total = targets.len();
    let mut out: Vec<(usize, CheckResult, bool)> = Vec::with_capacity(total);

    let mut stream =
        futures::stream::iter(targets.into_iter().enumerate().map(|(i, t)| async move {
            if let Some(hit) = state.cache.get(&t.domain).await {
                return (i, hit, true);
            }
            // The permit is taken around the lookup only, so a request waiting for
            // a slot is not holding one.
            let _permit = state
                .slots
                .acquire()
                .await
                .expect("the lookup semaphore is never closed");
            let result = check(&state.ctx, t.domain, t.tld).await;
            state.cache.put(&result).await;
            (i, result, false)
        }))
        .buffer_unordered(total.max(1));

    while let Some(item) = stream.next().await {
        out.push(item);
    }

    out.sort_by_key(|(i, _, _)| *i);
    let all_cached = !out.is_empty() && out.iter().all(|(_, _, cached)| *cached);
    (out.into_iter().map(|(_, r, _)| r).collect(), all_cached)
}

// --- request parsing -------------------------------------------------------

#[derive(Debug)]
struct Target {
    domain: String,
    tld: String,
}

/// Turn a query string into the list of domains to check.
///
/// Deliberately not routed through the CLI's `expand_names`/`--tld @file`
/// handling: those read files, and a network client must never be able to ask
/// this process to open one.
fn plan(state: &State, query: &str) -> Result<Vec<Target>, String> {
    let mut names = Vec::new();
    let mut tld_spec = String::new();

    for (key, value) in form_urlencoded::parse(query.as_bytes()) {
        match key.as_ref() {
            "name" => names.extend(split_list(&value)),
            "tld" => {
                for t in split_list(&value) {
                    if !tld_spec.is_empty() {
                        tld_spec.push(',');
                    }
                    tld_spec.push_str(&t);
                }
            }
            // Unknown parameters are ignored so that adding one later is not a
            // breaking change for callers that already send it.
            _ => {}
        }
    }

    if names.is_empty() {
        return Err("name is required, e.g. /v1/check?name=apple&tld=com".into());
    }
    if names.len() > state.max_lookups {
        return Err(format!(
            "too many names: {} asked for, {} is the limit",
            names.len(),
            state.max_lookups
        ));
    }

    let tlds = if tld_spec.is_empty() {
        None
    } else {
        if tld_spec.starts_with('@') {
            return Err("tld=@file is a command-line feature; list the TLDs instead".into());
        }
        let Selection { tlds, keyword } =
            resolve_tlds(&tld_spec, &state.ctx.registry, &state.ctx.bootstrap)
                .map_err(|e| format!("{e:#}"))?;

        // The CLI's rule, and there is no query parameter to change it: a list
        // `ds` chose itself (`all`, `rdap`, `popular`) drops the TLDs the
        // public cannot register in, and a TLD the caller named is checked
        // whatever it is — `tld=aws` comes back PRIVATE with the reason
        // attached rather than being silently dropped. That covers what
        // `--private include` is for on the command line, without letting a
        // caller spend hundreds of registry lookups on zones the bundled table
        // already knows the answer for.
        let tlds: Vec<String> = if keyword {
            tlds.into_iter()
                .filter(|t| !state.ctx.private.contains(t))
                .collect()
        } else {
            tlds
        };
        if tlds.is_empty() {
            return Err("nothing left to check: every TLD in that list is private".into());
        }
        Some(tlds)
    };

    let mut targets = Vec::new();
    let mut seen = std::collections::BTreeSet::new();

    for name in names {
        let domains: Vec<String> = match &tlds {
            // Same rule as the CLI: with no TLD list a dotted name is a whole
            // domain and a bare label means .com.
            None if name.contains('.') => vec![name.clone()],
            None => vec![format!("{name}.com")],
            Some(tlds) => tlds.iter().map(|t| format!("{name}.{t}")).collect(),
        };

        for domain in domains {
            let domain = to_domain(&domain)?;
            if !seen.insert(domain.clone()) {
                continue;
            }
            if targets.len() == state.max_lookups {
                return Err(format!(
                    "too many domains for one request; {} is the limit",
                    state.max_lookups
                ));
            }
            let tld = domain
                .split_once('.')
                .map(|(_, t)| t.to_string())
                .unwrap_or_default();
            targets.push(Target { domain, tld });
        }
    }

    Ok(targets)
}

fn split_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|v| v.trim().trim_matches('.').to_ascii_lowercase())
        .filter(|v| !v.is_empty())
        .collect()
}

/// Normalise and vet one name from the wire.
///
/// The vetting matters more here than it does on a command line: a WHOIS query
/// is a bare line written to a socket, so a name carrying a newline would be
/// two queries rather than one. IDNA rejects control characters already; this
/// is the second lock on the same door, and it also keeps the obviously
/// impossible (over-long labels, empty labels) from costing a network round
/// trip.
fn to_domain(name: &str) -> Result<String, String> {
    let ascii = idna::domain_to_ascii(name).map_err(|_| format!("invalid domain: {name}"))?;

    if ascii.is_empty() || ascii.len() > 253 {
        return Err(format!("invalid domain: {name}"));
    }
    if !ascii.contains('.') {
        return Err(format!("{name} has no TLD; pass tld= or a full domain"));
    }
    for label in ascii.split('.') {
        if label.is_empty() || label.len() > 63 {
            return Err(format!("invalid domain: {name}"));
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(format!("invalid domain: {name}"));
        }
        if !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(format!("invalid domain: {name}"));
        }
    }

    Ok(ascii)
}

// --- cache -----------------------------------------------------------------

struct Cache {
    ttl: Duration,
    entries: Mutex<HashMap<String, (Instant, CheckResult)>>,
}

impl Cache {
    fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: Mutex::new(HashMap::new()),
        }
    }

    fn ttl_for(&self, status: Status) -> Duration {
        match status {
            Status::Unknown => self.ttl.min(UNKNOWN_TTL),
            _ => self.ttl,
        }
    }

    async fn get(&self, domain: &str) -> Option<CheckResult> {
        if self.ttl.is_zero() {
            return None;
        }
        let entries = self.entries.lock().await;
        let (at, result) = entries.get(domain)?;
        (at.elapsed() < self.ttl_for(result.status)).then(|| result.clone())
    }

    async fn put(&self, result: &CheckResult) {
        if self.ttl.is_zero() {
            return;
        }
        let mut entries = self.entries.lock().await;
        if entries.len() >= CACHE_MAX {
            let ttl = self.ttl;
            let unknown = self.ttl.min(UNKNOWN_TTL);
            entries.retain(|_, (at, r)| {
                at.elapsed()
                    < match r.status {
                        Status::Unknown => unknown,
                        _ => ttl,
                    }
            });
            // Nothing had expired: drop the lot rather than grow without a
            // bound. A cache miss costs a lookup, not a wrong answer.
            if entries.len() >= CACHE_MAX {
                entries.clear();
            }
        }
        entries.insert(result.domain.clone(), (Instant::now(), result.clone()));
    }
}

// --- per-client rate limit -------------------------------------------------

struct Window {
    start: Instant,
    hits: u32,
}

/// A fixed window per client address.
///
/// The key is the address the connection came from, and nothing else:
/// `X-Forwarded-For` is a header anyone can write, so trusting it would hand
/// every client an unlimited supply of identities. Behind a reverse proxy the
/// limit is therefore the proxy's, and the proxy is where a per-user limit
/// belongs.
struct RateLimiter {
    per_minute: u32,
    seen: Mutex<HashMap<IpAddr, Window>>,
}

impl RateLimiter {
    fn new(per_minute: u32) -> Self {
        Self {
            per_minute,
            seen: Mutex::new(HashMap::new()),
        }
    }

    /// `Ok(())` to serve the request, `Err(seconds)` to refuse it.
    async fn admit(&self, client: IpAddr) -> Result<(), u64> {
        if self.per_minute == 0 {
            return Ok(());
        }
        const WINDOW: Duration = Duration::from_secs(60);

        let mut seen = self.seen.lock().await;
        if seen.len() >= CLIENTS_MAX {
            seen.retain(|_, w| w.start.elapsed() < WINDOW);
        }

        let window = seen.entry(client).or_insert(Window {
            start: Instant::now(),
            hits: 0,
        });
        if window.start.elapsed() >= WINDOW {
            window.start = Instant::now();
            window.hits = 0;
        }
        if window.hits >= self.per_minute {
            let left = WINDOW.saturating_sub(window.start.elapsed());
            return Err(left.as_secs() + 1);
        }
        window.hits += 1;
        Ok(())
    }
}

// --- responses -------------------------------------------------------------

fn json(
    status: StatusCode,
    body: &serde_json::Value,
    cache_control: &str,
) -> Response<Full<Bytes>> {
    let text = serde_json::to_string(body).unwrap_or_else(|_| "{}".into());
    Response::builder()
        .status(status)
        .header(CONTENT_TYPE, "application/json; charset=utf-8")
        .header(CACHE_CONTROL, cache_control)
        .body(Full::new(Bytes::from(format!("{text}\n"))))
        .expect("a response with static headers")
}

fn error(status: StatusCode, message: &str) -> Response<Full<Bytes>> {
    json(status, &serde_json::json!({ "error": message }), "no-store")
}

fn with_cors(mut response: Response<Full<Bytes>>, state: &State) -> Response<Full<Bytes>> {
    if let Some(origin) = &state.cors {
        if let Ok(value) = origin.parse() {
            response
                .headers_mut()
                .insert("access-control-allow-origin", value);
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Method as LookupMethod;

    fn state(max_lookups: usize, cache_ttl: u64, rate: u32) -> State {
        let timeout = Duration::from_secs(5);
        let resolver = dns::connect_resolver(timeout);
        State {
            ctx: Arc::new(Ctx {
                opts: Lookup::default(),
                client: reqwest::Client::builder()
                    .build()
                    .expect("a default client builds"),
                limiter: HostLimiter::new(4),
                bootstrap: Bootstrap::default(),
                registry: Registry::load().expect("the bundled table loads"),
                prices: Prices::default(),
                private: PrivateTlds::load().expect("the bundled table loads"),
                rules: Rules::load().expect("the bundled table loads"),
                resolver,
                records: None,
                timeout,
                tld_info: Mutex::new(HashMap::new()),
            }),
            cache: Cache::new(Duration::from_secs(cache_ttl)),
            rate: RateLimiter::new(rate),
            slots: Semaphore::new(4),
            max_lookups,
            cors: None,
        }
    }

    fn result(domain: &str, status: Status) -> CheckResult {
        CheckResult {
            domain: domain.into(),
            tld: domain
                .split_once('.')
                .map(|(_, t)| t.into())
                .unwrap_or_default(),
            status,
            price: None,
            method: LookupMethod::Rdap,
            registry: None,
            whois_server: None,
            note: None,
            details: None,
            whois_raw: None,
            rdap_raw: None,
            dns: None,
            register: None,
            elapsed_ms: 1,
        }
    }

    async fn body_of(response: Response<Full<Bytes>>) -> serde_json::Value {
        use http_body_util::BodyExt;
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("a full body never fails")
            .to_bytes();
        serde_json::from_slice(&bytes).expect("the body is JSON")
    }

    fn get(path_and_query: &str) -> Request<()> {
        Request::builder()
            .method(Method::GET)
            .uri(path_and_query)
            .body(())
            .expect("a valid request")
    }

    const CLIENT: IpAddr = IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);

    #[test]
    fn expands_names_across_tlds_in_order() {
        let s = state(50, 0, 0);
        let targets = plan(&s, "name=apple,orange&tld=com,net").unwrap();
        let domains: Vec<&str> = targets.iter().map(|t| t.domain.as_str()).collect();
        assert_eq!(
            domains,
            ["apple.com", "apple.net", "orange.com", "orange.net"]
        );
        assert_eq!(targets[1].tld, "net");
    }

    #[test]
    fn repeated_parameters_add_up_and_duplicates_collapse() {
        let s = state(50, 0, 0);
        let targets = plan(&s, "name=apple&name=apple&name=orange&tld=com").unwrap();
        assert_eq!(targets.len(), 2);
    }

    #[test]
    fn a_bare_name_means_dot_com_and_a_dotted_one_is_taken_as_given() {
        let s = state(50, 0, 0);
        assert_eq!(plan(&s, "name=apple").unwrap()[0].domain, "apple.com");
        let uk = plan(&s, "name=apple.co.uk").unwrap();
        assert_eq!(uk[0].domain, "apple.co.uk");
        assert_eq!(uk[0].tld, "co.uk");
    }

    #[test]
    fn unicode_names_are_punycoded() {
        let s = state(50, 0, 0);
        let targets = plan(&s, "name=b%C3%BCcher&tld=de").unwrap();
        assert_eq!(targets[0].domain, "xn--bcher-kva.de");
    }

    #[test]
    fn a_request_without_a_name_is_rejected() {
        let s = state(50, 0, 0);
        assert!(plan(&s, "").is_err());
        assert!(plan(&s, "tld=com").is_err());
    }

    #[test]
    fn the_lookup_cap_holds_for_names_and_for_tlds() {
        let s = state(4, 0, 0);
        assert!(plan(&s, "name=apple&tld=com,net,org,io").is_ok());
        assert!(plan(&s, "name=apple&tld=com,net,org,io,co").is_err());
        assert!(plan(&s, "name=a,b,c&tld=com,net").is_err());
        // `all` is thousands of TLDs, so the cap is what rules it out.
        assert!(plan(&s, "name=apple&tld=all").is_err());
    }

    /// The CLI prunes private TLDs out of the lists it chose itself and leaves
    /// the ones the user named alone. The API has no parameter to change that,
    /// so both halves are pinned here.
    #[test]
    fn keyword_lists_drop_private_tlds_and_named_ones_survive() {
        // The cap has to be lifted to see a keyword list at all; `all` is the
        // bundled WHOIS table, which carries exactly one private TLD (.arpa).
        let s = state(2000, 0, 0);
        let all = plan(&s, "name=apple&tld=all").unwrap();
        assert!(all.len() > 100, "the bundled table should be large");
        assert!(
            !all.iter().any(|t| t.tld == "arpa"),
            "a keyword list must not sweep private TLDs"
        );

        // Naming one is a request to check it, and it is answered.
        let named = plan(&s, "name=login&tld=arpa,com").unwrap();
        let tlds: Vec<&str> = named.iter().map(|t| t.tld.as_str()).collect();
        assert_eq!(tlds, ["arpa", "com"]);
    }

    #[test]
    fn file_backed_tld_lists_are_not_reachable_over_http() {
        let s = state(50, 0, 0);
        let err = plan(&s, "name=apple&tld=%40/etc/passwd").unwrap_err();
        assert!(err.contains("command-line feature"), "{err}");
    }

    #[test]
    fn names_that_would_break_a_whois_query_are_refused() {
        // A newline would turn one port-43 query into two.
        assert!(to_domain("apple\r\nhelp.com").is_err());
        assert!(to_domain("apple .com").is_err());
        assert!(to_domain("apple..com").is_err());
        assert!(to_domain("-apple.com").is_err());
        assert!(to_domain(&format!("{}.com", "a".repeat(64))).is_err());
        assert!(to_domain("apple").is_err(), "a bare label has no TLD");
        assert_eq!(to_domain("APPLE.COM").unwrap(), "apple.com");
    }

    #[tokio::test]
    async fn the_cache_returns_what_it_was_given() {
        let c = Cache::new(Duration::from_secs(300));
        c.put(&result("apple.com", Status::Taken)).await;
        let hit = c.get("apple.com").await.expect("a fresh entry");
        assert_eq!(hit.status, Status::Taken);
        assert!(c.get("orange.com").await.is_none());
    }

    #[tokio::test]
    async fn an_unknown_is_cached_for_much_less_time_than_an_answer() {
        let c = Cache::new(Duration::from_secs(3600));
        assert_eq!(c.ttl_for(Status::Unknown), UNKNOWN_TTL);
        assert_eq!(c.ttl_for(Status::Available), Duration::from_secs(3600));

        // ...and it comes back an unknown, never an available.
        c.put(&result("apple.com", Status::Unknown)).await;
        assert_eq!(
            c.get("apple.com").await.map(|r| r.status),
            Some(Status::Unknown)
        );
    }

    #[tokio::test]
    async fn a_zero_ttl_turns_the_cache_off() {
        let c = Cache::new(Duration::ZERO);
        c.put(&result("apple.com", Status::Taken)).await;
        assert!(c.get("apple.com").await.is_none());
    }

    #[tokio::test]
    async fn the_rate_limiter_refuses_the_client_over_its_budget() {
        let l = RateLimiter::new(2);
        assert!(l.admit(CLIENT).await.is_ok());
        assert!(l.admit(CLIENT).await.is_ok());
        let retry = l.admit(CLIENT).await.unwrap_err();
        assert!((1..=61).contains(&retry), "retry-after was {retry}");

        // Another client has its own budget.
        let other = IpAddr::V4(std::net::Ipv4Addr::new(203, 0, 113, 7));
        assert!(l.admit(other).await.is_ok());

        // ...and zero means no limit at all.
        let off = RateLimiter::new(0);
        for _ in 0..1000 {
            assert!(off.admit(CLIENT).await.is_ok());
        }
    }

    #[tokio::test]
    async fn health_answers_without_touching_the_network() {
        let s = state(50, 0, 0);
        let response = handle(&s, CLIENT, get("/healthz")).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_of(response).await["status"], "ok");
    }

    #[tokio::test]
    async fn unknown_paths_and_methods_are_refused_in_json() {
        let s = state(50, 0, 0);

        let response = handle(&s, CLIENT, get("/v2/check?name=apple")).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(body_of(response).await["error"].is_string());

        let post = Request::builder()
            .method(Method::POST)
            .uri("/v1/check")
            .body(())
            .unwrap();
        let response = handle(&s, CLIENT, post).await;
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert_eq!(response.headers()[ALLOW], "GET");
    }

    #[tokio::test]
    async fn a_bad_request_never_reaches_a_registry() {
        let s = state(50, 0, 0);
        let response = handle(&s, CLIENT, get("/v1/check")).await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert!(body_of(response).await["error"]
            .as_str()
            .unwrap()
            .contains("name is required"));
    }

    #[tokio::test]
    async fn a_throttled_client_is_told_when_to_come_back() {
        let s = state(50, 0, 1);
        // The first request is admitted and then fails on its parameters,
        // which is enough to prove the limiter ran before the lookup did.
        assert_eq!(
            handle(&s, CLIENT, get("/v1/check")).await.status(),
            StatusCode::BAD_REQUEST
        );
        let response = handle(&s, CLIENT, get("/v1/check?name=apple")).await;
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(response.headers().contains_key(RETRY_AFTER));
    }

    #[tokio::test]
    async fn cached_results_are_served_without_a_lookup() {
        let s = state(50, 300, 0);
        // Priming the cache is the only way this test can answer at all: it
        // has no network, so a miss would hang or fail rather than return.
        s.cache.put(&result("apple.com", Status::Taken)).await;
        let response = handle(&s, CLIENT, get("/v1/check?name=apple&tld=com")).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["x-cache"], "HIT");

        let body = body_of(response).await;
        assert_eq!(body[0]["domain"], "apple.com");
        assert_eq!(body[0]["status"], "taken");
    }

    #[tokio::test]
    async fn an_unknown_reaches_the_client_as_unknown() {
        let s = state(50, 300, 0);
        s.cache.put(&result("apple.com", Status::Unknown)).await;
        let response = handle(&s, CLIENT, get("/v1/check?name=apple&tld=com")).await;
        let cache_control = response.headers()[CACHE_CONTROL].clone();
        let body = body_of(response).await;
        assert_eq!(body[0]["status"], "unknown");
        // An unknown is a lookup that failed, not an answer to store.
        assert_eq!(cache_control, "no-store");
    }

    #[tokio::test]
    async fn a_private_tld_reaches_the_client_as_private() {
        let s = state(50, 300, 0);
        s.cache.put(&result("login.aws", Status::Private)).await;
        let response = handle(&s, CLIENT, get("/v1/check?name=login&tld=aws")).await;
        // Unlike an unknown, a private TLD is a settled answer out of a
        // bundled table, so it is as cacheable as a taken one.
        assert_eq!(response.headers()[CACHE_CONTROL], "public, max-age=300");

        let body = body_of(response).await;
        assert_eq!(body[0]["domain"], "login.aws");
        assert_eq!(body[0]["status"], "private");
        assert_ne!(body[0]["status"], "available");
    }

    /// A private TLD reaches a lookup by one route only — the caller naming
    /// it — and `check` marks everything it finds there PRIVATE. So no
    /// parameter combination can produce a private TLD the caller did not ask
    /// for, and none can produce one reported as available.
    #[test]
    fn a_private_tld_is_only_ever_one_the_caller_named() {
        let s = state(2000, 0, 0);

        for keyword in ["name=login&tld=all", "name=login&tld=popular"] {
            let private: Vec<String> = plan(&s, keyword)
                .unwrap()
                .into_iter()
                .filter(|t| s.ctx.private.contains(&t.tld))
                .map(|t| t.domain)
                .collect();
            assert!(private.is_empty(), "{keyword} swept {private:?}");
        }

        // Named outright, or written out as a whole domain: both are the
        // caller asking, and both are answered.
        assert_eq!(plan(&s, "name=login&tld=aws").unwrap()[0].tld, "aws");
        assert_eq!(plan(&s, "name=login.aws").unwrap()[0].tld, "aws");
    }

    #[tokio::test]
    async fn cors_headers_are_sent_only_when_configured() {
        let mut s = state(50, 0, 0);
        assert!(!handle(&s, CLIENT, get("/healthz"))
            .await
            .headers()
            .contains_key("access-control-allow-origin"));

        s.cors = Some("https://ds.aminul.dev".into());
        let response = handle(&s, CLIENT, get("/healthz")).await;
        assert_eq!(
            response.headers()["access-control-allow-origin"],
            "https://ds.aminul.dev"
        );
    }
}
