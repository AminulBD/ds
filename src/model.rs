//! Shared result types.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Available,
    Taken,
    Unknown,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Available => "AVAILABLE",
            Status::Taken => "TAKEN",
            Status::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    Rdap,
    Whois,
    None,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Rdap => "rdap",
            Method::Whois => "whois",
            Method::None => "-",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Details {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registrar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registrar_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registrant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub statuses: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub nameservers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dnssec: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub abuse_email: Option<String>,
}

impl Details {
    pub fn is_empty(&self) -> bool {
        self.registrar.is_none()
            && self.registrant.is_none()
            && self.created.is_none()
            && self.updated.is_none()
            && self.expires.is_none()
            && self.statuses.is_empty()
            && self.nameservers.is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DnsRecords {
    pub records: Vec<(String, Vec<String>)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    pub domain: String,
    pub tld: String,
    pub status: Status,
    pub method: Method,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub registry: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub whois_server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Details>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub whois_raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rdap_raw: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dns: Option<DnsRecords>,
    pub elapsed_ms: u64,
}
