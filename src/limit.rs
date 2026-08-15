//! Per-host concurrency limiting, adaptive pacing and a circuit breaker.
//!
//! Hundreds of TLDs share a handful of RDAP/WHOIS servers (Identity Digital
//! alone runs ~250 gTLDs), so a global concurrency cap is not enough: without a
//! per-host cap a large `--tld all` run gets 403/429-ed or has its WHOIS
//! connections reset. When a host does start pushing back we slow down for that
//! host only, and after repeated refusals stop querying it altogether rather
//! than spending the rest of the run on doomed retries.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

/// Consecutive refusals from one host before we stop querying it.
const TRIP_AFTER: u32 = 6;
/// How long a tripped host is left alone before one probe is allowed through.
const COOL_OFF: Duration = Duration::from_secs(30);
const MAX_INTERVAL: Duration = Duration::from_millis(1500);

struct Gate {
    /// Earliest time the next request to this host may start.
    next_at: Instant,
    /// Minimum spacing between requests; grows while the host pushes back.
    interval: Duration,
    refusals: u32,
    /// While set and in the future, the host is skipped entirely.
    open_until: Option<Instant>,
}

struct Host {
    sem: Arc<Semaphore>,
    gate: Arc<Mutex<Gate>>,
}

pub struct HostLimiter {
    per_host: usize,
    hosts: Mutex<HashMap<String, Host>>,
}

impl HostLimiter {
    pub fn new(per_host: usize) -> Self {
        Self {
            per_host: per_host.max(1),
            hosts: Mutex::new(HashMap::new()),
        }
    }

    async fn host(&self, host: &str) -> (Arc<Semaphore>, Arc<Mutex<Gate>>) {
        let mut hosts = self.hosts.lock().await;
        let entry = hosts
            .entry(host.to_ascii_lowercase())
            .or_insert_with(|| Host {
                sem: Arc::new(Semaphore::new(self.per_host)),
                gate: Arc::new(Mutex::new(Gate {
                    next_at: Instant::now(),
                    interval: Duration::ZERO,
                    refusals: 0,
                    open_until: None,
                })),
            });
        (entry.sem.clone(), entry.gate.clone())
    }

    /// Wait for a slot on `host`, respecting any pacing currently in force.
    pub async fn acquire(&self, host: &str) -> OwnedSemaphorePermit {
        let (sem, gate) = self.host(host).await;
        let permit = sem
            .acquire_owned()
            .await
            .expect("host semaphore is never closed");

        loop {
            let wait = {
                let mut g = gate.lock().await;
                let now = Instant::now();
                if g.next_at > now {
                    g.next_at - now
                } else {
                    g.next_at = now + g.interval;
                    Duration::ZERO
                }
            };
            if wait.is_zero() {
                break;
            }
            tokio::time::sleep(wait).await;
        }

        permit
    }

    /// The host refused us (rate limit, reset connection): back off.
    pub async fn refused(&self, host: &str) {
        let (_, gate) = self.host(host).await;
        let mut g = gate.lock().await;
        g.refusals += 1;
        g.interval = (g.interval * 2)
            .max(Duration::from_millis(250))
            .min(MAX_INTERVAL);
        let now = Instant::now();
        g.next_at = now + g.interval;
        if g.refusals >= TRIP_AFTER {
            g.open_until = Some(now + COOL_OFF);
        }
    }

    /// The host answered: forget the back-off.
    pub async fn answered(&self, host: &str) {
        let (_, gate) = self.host(host).await;
        let mut g = gate.lock().await;
        g.refusals = 0;
        g.interval = Duration::ZERO;
        g.open_until = None;
    }

    /// True once a host has refused us often enough that further queries are
    /// a waste of time (and would only extend its lockout). After the cool-off
    /// one probe is let through: if it succeeds the host is back in play.
    pub async fn tripped(&self, host: &str) -> bool {
        let (_, gate) = self.host(host).await;
        let mut g = gate.lock().await;
        match g.open_until {
            Some(until) if Instant::now() < until => true,
            Some(_) => {
                g.open_until = None;
                g.refusals = TRIP_AFTER - 1;
                false
            }
            None => false,
        }
    }
}

/// `https://rdap.example.com/rdap/` -> `rdap.example.com`
pub fn host_of(url: &str) -> &str {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let end = rest.find(['/', ':', '?']).unwrap_or(rest.len());
    &rest[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_hosts() {
        assert_eq!(
            host_of("https://rdap.verisign.com/com/v1/"),
            "rdap.verisign.com"
        );
        assert_eq!(host_of("http://a.b.c:8080/x"), "a.b.c");
        assert_eq!(host_of("whois.nic.io"), "whois.nic.io");
    }

    #[tokio::test]
    async fn trips_after_repeated_refusals() {
        let l = HostLimiter::new(2);
        for _ in 0..TRIP_AFTER {
            assert!(!l.tripped("h").await);
            l.refused("h").await;
        }
        assert!(l.tripped("h").await);

        l.answered("h").await;
        assert!(!l.tripped("h").await);
    }

    #[tokio::test]
    async fn probes_again_after_cool_off() {
        let l = HostLimiter::new(2);
        for _ in 0..TRIP_AFTER {
            l.refused("h").await;
        }
        assert!(l.tripped("h").await);

        // Expire the cool-off by hand rather than sleeping for it.
        {
            let (_, gate) = l.host("h").await;
            gate.lock().await.open_until = Some(Instant::now() - Duration::from_secs(1));
        }
        assert!(!l.tripped("h").await, "one probe should be let through");
        l.refused("h").await;
        assert!(l.tripped("h").await, "a failed probe re-trips immediately");
    }
}
