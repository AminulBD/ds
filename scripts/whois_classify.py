"""A faithful Python port of the response classifier in `src/whois.rs`.

`refresh-whois.py` decides whether a candidate server/needle pair is safe to
put in `whois.json` by running the *real* classifier over the *real* responses
the registry gave it. Keeping that logic in one place, mirroring the Rust
line for line, is what makes the verdicts in the report mean anything.

Whenever `classify()` in `src/whois.rs` changes, change this too — the test
`scripts/test_whois_classify.py` pins the shared cases from the Rust tests.
"""

AVAILABLE = "available"
TAKEN = "taken"
UNKNOWN = "unknown"

# --- src/whois.rs: TAKEN_MARKERS -------------------------------------------
TAKEN_MARKERS = [
    "domain name:",
    "domain:",
    "registrar:",
    "creation date",
    "created:",
    "registered on",
    "expiry date",
    "expiration date",
    "registry expiry date",
    "name server",
    "nserver",
    "status: connect",
    "status: active",
    "already registered",
    "can not be registered",
    "cannot be registered",
    "is not available for registration",
]

# --- src/whois.rs: STRONG_FREE_MARKERS -------------------------------------
STRONG_FREE_MARKERS = [
    "no match for",
    "no matching record",
    "no object found",
    "does not exist",
    "no such domain",
    "domain not found",
    "no entries found",
    "no data found",
    "not registered",
    "status: free",
    "status: available",
    "available for registration",
]

# --- src/whois.rs: FREE_MARKERS --------------------------------------------
FREE_MARKERS = [
    "no match",
    "not found",
    "is available",
    "no information available",
    "nothing found",
]

# --- src/whois.rs: REFUSAL_MARKERS -----------------------------------------
REFUSAL_MARKERS = [
    "tld is not supported",
    "tld not supported",
    "not supported by this registry",
    "this tld has no whois server",
    "no whois server is known",
    "access denied",
    "requests of this client are not permitted",
    "queries are not permitted",
    "status: not allowed",
    "use only approved characters",
    "domain status: error",
    "not allowed for this domain category",
]

# --- src/whois.rs: ERROR_MARKERS -------------------------------------------
ERROR_MARKERS = [
    "limit exceeded",
    "rate limit",
    "too many requests",
    "try again later",
    "quota exceeded",
    "connection refused",
    "service unavailable",
    "temporarily unavailable",
    "blocked",
]


def normalize(raw: str) -> str:
    """Lowercase; tabs become spaces and runs of spaces collapse to one."""
    out = []
    last_space = False
    for c in raw:
        if c == "\t":
            c = " "
        if c == " ":
            if not last_space:
                out.append(" ")
            last_space = True
        else:
            last_space = False
            out.append(c.lower())
    return "".join(out)


def contains_unnegated(haystack: str, needle: str) -> bool:
    """`needle` appears somewhere it is not being negated. Mirrors Rust."""
    if not needle:
        return False
    from_ = 0
    while True:
        at = haystack.find(needle, from_)
        if at < 0:
            return False
        before = haystack[:at]
        trimmed = before.rstrip()
        # Rust: rsplit on "not alphanumeric and not apostrophe", take first.
        last_word = ""
        for i in range(len(trimmed) - 1, -1, -1):
            ch = trimmed[i]
            if ch.isalnum() or ch == "'":
                last_word = ch + last_word
            else:
                break
        negated = (
            last_word in ("not", "no", "isn't", "cannot", "never")
            or before.endswith("un")
            or before.endswith("non")
        )
        if not negated:
            return True
        from_ = at + max(len(needle), 1)


def answered_name(raw: str):
    """The domain a record is about, if the *first* `key: value` line names one.

    Mirrors the Rust exactly, including its early return: `split_once(':')?`
    inside the loop means a line with no colon ends the search.
    """
    for line in raw.split("\n"):
        line = line.strip()
        if ":" not in line:
            return None
        key, value = line.split(":", 1)
        key = key.strip().lower()
        if key in ("domain name", "domain", "domainname", "internationalized domain name"):
            value = value.strip().rstrip(".").lower()
            if value and "." in value:
                return value
    return None


def classify(raw: str, needle: str, domain: str):
    """-> (status, note). A port of `classify` in src/whois.rs."""
    lower = normalize(raw)
    needle_lower = normalize(needle)

    if needle_lower and contains_unnegated(lower, needle_lower):
        return AVAILABLE, None

    for m in REFUSAL_MARKERS:
        if m in lower:
            return UNKNOWN, "whois said: " + m

    for m in ERROR_MARKERS:
        if m in lower:
            if not any(t in lower for t in TAKEN_MARKERS):
                return UNKNOWN, "whois said: " + m
            break

    for m in STRONG_FREE_MARKERS:
        if contains_unnegated(lower, m):
            return AVAILABLE, None

    answered = answered_name(raw)
    if answered is not None and answered != domain.lower():
        return UNKNOWN, "whois answered for " + answered

    terse = len(lower.strip()) < 40
    if terse and ("not available" in lower or "unavailable" in lower):
        return TAKEN, None

    for m in TAKEN_MARKERS:
        if m in lower:
            return TAKEN, None

    for m in FREE_MARKERS:
        if contains_unnegated(lower, m):
            return AVAILABLE, None

    if not lower.strip():
        return UNKNOWN, "empty whois response"

    return UNKNOWN, "unrecognised whois response"


def format_query(host: str, domain: str) -> str:
    """Mirrors `format_query` in src/whois.rs."""
    h = host.lower()
    if h.endswith("verisign-grs.com") or h.endswith("crsnic.net") or h.endswith("internic.net"):
        return "domain %s\r\n" % domain
    if h.endswith("denic.de"):
        return "-T dn %s\r\n" % domain
    if h.endswith("jprs.jp"):
        return "%s/e\r\n" % domain
    if h.endswith("arnes.si") or h.endswith("dk-hostmaster.dk"):
        return "--show-handles %s\r\n" % domain
    return "%s\r\n" % domain
