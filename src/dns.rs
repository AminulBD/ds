//! DNS: the resolver every outbound connection goes through, and the record
//! lookups behind `--dns-records`.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use hickory_resolver::config::{NameServerConfigGroup, ResolverConfig, ResolverOpts};
use hickory_resolver::proto::rr::RecordType;
use hickory_resolver::TokioAsyncResolver;

use crate::model::DnsRecords;

const TYPES: &[RecordType] = &[
    RecordType::A,
    RecordType::AAAA,
    RecordType::NS,
    RecordType::MX,
    RecordType::TXT,
    RecordType::CNAME,
    RecordType::SOA,
];

pub fn resolver(timeout: Duration) -> TokioAsyncResolver {
    let mut opts = ResolverOpts::default();
    opts.timeout = timeout;
    opts.attempts = 1;
    // Cloudflare's resolver: consistent behaviour regardless of the local setup.
    TokioAsyncResolver::tokio(ResolverConfig::cloudflare(), opts)
}

/// The resolver used to reach RDAP and WHOIS servers.
///
/// The platform resolver is not always usable: a static musl binary carries no
/// NSS, and Android — where Termux runs — has no `/etc/resolv.conf` at all, so
/// `getaddrinfo` fails there with EAI_AGAIN ("Try again") and every lookup is
/// reported UNKNOWN. Resolving in process avoids it. The system configuration
/// is used whenever there is one, so split-horizon DNS and `/etc/hosts` keep
/// working; only when there is none do the public resolvers answer.
pub fn connect_resolver(timeout: Duration) -> TokioAsyncResolver {
    let (config, mut opts) = connect_config(hickory_resolver::system_conf::read_system_conf().ok());
    // A single DNS try has to fit inside the per-request budget with room left
    // for another server and for the connection itself.
    opts.timeout = (timeout / 4).max(Duration::from_secs(2));
    opts.attempts = 2;
    // Every server in the list is configured for UDP and TCP both, but the
    // TCP entries are only ever reached with this on. Networks that black-hole
    // UDP port 53 — and the Android emulator, which mangles it — resolve
    // nothing without it.
    opts.try_tcp_on_error = true;
    TokioAsyncResolver::tokio(config, opts)
}

/// Whatever the system configured, or the public resolvers when it configured
/// nothing usable.
fn connect_config(
    system: Option<(ResolverConfig, ResolverOpts)>,
) -> (ResolverConfig, ResolverOpts) {
    match system {
        Some((config, opts)) if !config.name_servers().is_empty() => (config, opts),
        _ => {
            let mut servers = NameServerConfigGroup::cloudflare();
            // Two providers, so one being blocked or unreachable is not fatal.
            servers.merge(NameServerConfigGroup::google());
            (
                ResolverConfig::from_parts(None, Vec::new(), servers),
                ResolverOpts::default(),
            )
        }
    }
}

/// Hands that resolver to reqwest, which otherwise calls `getaddrinfo` on a
/// blocking thread and hits the same wall.
pub struct Dns(TokioAsyncResolver);

impl Dns {
    pub fn new(resolver: &TokioAsyncResolver) -> Arc<Self> {
        Arc::new(Self(resolver.clone()))
    }
}

impl reqwest::dns::Resolve for Dns {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let resolver = self.0.clone();
        Box::pin(async move {
            let found = resolver.lookup_ip(name.as_str()).await?;
            // Port 0: reqwest fills in the one from the URL.
            let addrs: reqwest::dns::Addrs =
                Box::new(found.into_iter().map(|ip| SocketAddr::new(ip, 0)));
            Ok(addrs)
        })
    }
}

pub async fn lookup(resolver: &TokioAsyncResolver, domain: &str) -> DnsRecords {
    let mut records = Vec::new();

    for &rtype in TYPES {
        let answer = match resolver.lookup(domain, rtype).await {
            Ok(a) => a,
            Err(_) => continue,
        };
        let mut values: Vec<String> = answer
            .record_iter()
            .filter(|r| r.record_type() == rtype)
            .map(|r| r.data().map(|d| d.to_string()).unwrap_or_default())
            .filter(|s| !s.is_empty())
            .collect();
        values.sort();
        values.dedup();
        if !values.is_empty() {
            records.push((rtype.to_string(), values));
        }
    }

    DnsRecords { records }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn servers(config: &ResolverConfig) -> Vec<IpAddr> {
        let mut ips: Vec<IpAddr> = config
            .name_servers()
            .iter()
            .map(|n| n.socket_addr.ip())
            .collect();
        ips.sort();
        ips.dedup();
        ips
    }

    /// Termux/Android: no /etc/resolv.conf to read at all.
    #[test]
    fn falls_back_to_public_resolvers_without_a_system_config() {
        let (config, _) = connect_config(None);
        let ips = servers(&config);
        assert!(
            ips.contains(&IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1))),
            "{ips:?}"
        );
        assert!(
            ips.contains(&IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))),
            "{ips:?}"
        );
    }

    /// A resolv.conf that parses but lists no nameserver is just as unusable.
    #[test]
    fn falls_back_when_the_system_config_names_no_servers() {
        let empty = ResolverConfig::from_parts(None, Vec::new(), NameServerConfigGroup::new());
        let (config, _) = connect_config(Some((empty, ResolverOpts::default())));
        assert!(!servers(&config).is_empty());
    }

    #[test]
    fn keeps_the_system_resolvers_when_there_are_any() {
        let quad9 = IpAddr::V4(Ipv4Addr::new(9, 9, 9, 9));
        let system = ResolverConfig::from_parts(
            None,
            Vec::new(),
            NameServerConfigGroup::from_ips_clear(&[quad9], 53, true),
        );
        let (config, _) = connect_config(Some((system, ResolverOpts::default())));
        assert_eq!(servers(&config), vec![quad9]);
    }
}
