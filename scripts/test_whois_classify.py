#!/usr/bin/env python3
"""Parity checks for `whois_classify.py` against the tests in `src/whois.rs`.

The Python classifier is what decides whether a harvested server is safe to add
to `whois.json`, so it has to agree with the Rust one. Every case below is
lifted from `mod tests` in `src/whois.rs`; if a case here starts failing, the
port has drifted from the real implementation and the harvest is no longer
telling the truth.

    python3 scripts/test_whois_classify.py
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from whois_classify import (  # noqa: E402
    AVAILABLE,
    TAKEN,
    UNKNOWN,
    classify,
    contains_unnegated,
    format_query,
    normalize,
)

CASES = [
    # (name, raw, needle, domain, expected)
    ("needle_match_wins", "Domain not found.\n", "Domain not found", "x.test", AVAILABLE),
    (
        "registered_record_is_taken",
        "Domain Name: apple.com\nRegistrar: COM LAUDE\nCreation Date: 1987-02-19\n",
        "No match for",
        "apple.com",
        TAKEN,
    ),
    (
        "terse_ch_style_available",
        "1: This domain name can be registered.",
        "1:",
        "x.test",
        AVAILABLE,
    ),
    (
        "terse_ch_style_taken",
        "0: This domain name can not be registered.",
        "1:",
        "x.test",
        TAKEN,
    ),
    ("generic_free_phrasing", "NOT FOUND\n", "no match", "x.test", AVAILABLE),
    ("dead_server_is_unknown", "TLD is not supported.\n", "No match", "x.test", UNKNOWN),
    (
        "refusal_that_echoes_the_query",
        "Domain Name: zqxwvu7391aminul.tattoo\n\n"
        "                   >>> This name is not available for registration:\n\n"
        "                   >>> Tld not supported by this registry interface\n",
        "No match",
        "x.test",
        UNKNOWN,
    ),
    (
        "echoed_name_with_no_object_found",
        "Domain: zqxwvu7391aminul.sr\nMessage: No Object Found\n"
        "Sponsoring Registrar: Datasur\n",
        "No match",
        "x.test",
        AVAILABLE,
    ),
    (
        "query_rejection_despite_tabs",
        "Domain:\tx.ac.be\nStatus:\tNOT ALLOWED\nMessage:\tUse only approved characters.\n",
        "Status: AVAILABLE",
        "x.test",
        UNKNOWN,
    ),
    (
        "teleinfo_no_matching_record",
        "Internationalized Domain Name: x.anquan\nNo matching record.\n",
        "No match for",
        "x.test",
        AVAILABLE,
    ),
    (
        "registry_error_status",
        "Domain Name: x.ac.bd\nDomain Status: Error\n"
        "Message: Reseller not allowed for this domain category\n",
        "No match",
        "x.test",
        UNKNOWN,
    ),
    (
        "record_about_the_parent_zone",
        "Domain Name: ernet.in\nRegistrar: National Informatics Centre\n"
        "Creation Date: 2005-02-19T06:12:14.398Z\n",
        "No match",
        "foo.ernet.in",
        UNKNOWN,
    ),
    (
        "boilerplate_is_not_a_refusal",
        "% Restricted rights.\n"
        "% It is not permitted to use this data for advertising.\n"
        "Domain: apple.de\nStatus: connect\n",
        "Status: free",
        "apple.de",
        TAKEN,
    ),
    ("negated_needle_available", "Available", "Available", "x.com.au", AVAILABLE),
    ("negated_needle_taken", "Not Available", "Available", "x.com.au", TAKEN),
    ("attached_negation", "Domain is unavailable", "available", "x.test", TAKEN),
    (
        "long_record_mentioning_withheld_data",
        "Domain Name: x.test\nRegistrar: Someone\n"
        "Registrant contact information is not available for privacy reasons.\n",
        "no match",
        "x.test",
        TAKEN,
    ),
]

UNNEGATED = [
    ("status: available", "available", True),
    ("not available", "available", False),
    ("unavailable", "available", False),
    ("domino available", "available", True),
]

QUERIES = [
    ("whois.verisign-grs.com", "apple.com", "domain apple.com\r\n"),
    ("whois.denic.de", "apple.de", "-T dn apple.de\r\n"),
    ("whois.jprs.jp", "apple.jp", "apple.jp/e\r\n"),
    ("whois.dk-hostmaster.dk", "apple.dk", "--show-handles apple.dk\r\n"),
    ("whois.nic.uk", "apple.uk", "apple.uk\r\n"),
    # A gTLD named after a registry must not inherit that registry's prefix.
    ("whois.nic.jprs", "nic.jprs", "nic.jprs\r\n"),
    ("whois.nic.denic", "nic.denic", "nic.denic\r\n"),
]


def main() -> int:
    failures = []

    for name, raw, needle, domain, expected in CASES:
        got, note = classify(raw, needle, domain)
        if got != expected:
            failures.append("%s: expected %s, got %s (%s)" % (name, expected, got, note))

    for haystack, needle, expected in UNNEGATED:
        got = contains_unnegated(normalize(haystack), normalize(needle))
        if got != expected:
            failures.append(
                "contains_unnegated(%r, %r): expected %s, got %s"
                % (haystack, needle, expected, got)
            )

    for host, domain, expected in QUERIES:
        got = format_query(host, domain)
        if got != expected:
            failures.append("format_query(%s): expected %r, got %r" % (host, expected, got))

    # `.in` answers a delegated second level with the parent's record; the note
    # has to name the domain that was actually answered for.
    _, note = classify("Domain Name: ernet.in\nRegistrar: NIC\n", "No match", "foo.ernet.in")
    if note != "whois answered for ernet.in":
        failures.append("parent-zone note: got %r" % note)

    total = len(CASES) + len(UNNEGATED) + len(QUERIES) + 1
    if failures:
        for f in failures:
            print("FAIL  " + f)
        print("\n%d/%d failed" % (len(failures), total))
        return 1
    print("ok — %d checks, all matching src/whois.rs" % total)
    return 0


if __name__ == "__main__":
    sys.exit(main())
