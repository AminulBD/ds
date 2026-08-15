//! DNS record lookups for `--dns-records`.

use std::time::Duration;

use hickory_resolver::config::{ResolverConfig, ResolverOpts};
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
