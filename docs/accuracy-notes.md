# Accuracy notes

* RDAP is authoritative: `404` means the name is not registered.
* "Not registered" and "you can register it" are not the same claim. In a brand
  or reserved TLD every lookup comes back unregistered forever, so those are
  reported `PRIVATE` — see [Brand and reserved
  TLDs](choosing-tlds.md#brand-and-reserved-tlds). Only Specification 13 brands and
  RFC-reserved names are called that; nothing else is assumed either way.
* WHOIS is text matching. The per-registry needle from `whois.json` is tried
  first, then generic markers. Anything unrecognised is `UNKNOWN` rather than a
  guess.
* Needle matching is negation-aware. auDA's availability service answers
  `Available` or `Not Available` and the bundled needle for `.au` is
  "Available", so a plain substring test reports every taken `.au` domain as
  free. Matches preceded by "not"/"no", or attached as in "unavailable", do not
  count.
* Four registry quirks are handled explicitly, because each one otherwise reads
  as a registration: answers that echo the queried name back before saying "no
  object found" (`.sr`, `.fj`), answers for a TLD the server does not serve
  (`.tattoo`, `.photo`), non-RDAP error bodies returned with HTTP 200 (`.sn`),
  and answers describing the *parent* zone instead of the name asked about
  (`foo.ernet.in` -> `ernet.in`).
* Some registries answer nobody: `.li` and `.qa` refuse public WHOIS and have no
  RDAP, and a few bundled WHOIS hosts no longer exist. Those come back `UNKNOWN`
  with the reason attached.
* Most of the bundled table is harvested from IANA and tested against the
  registry before being written — see
  [The bundled WHOIS table](the-bundled-whois-table.md). The older hand-written
  entries have not been through that mill.
* `--where` reports registrars from published prices, so it under-claims rather
  than over-claims: a registrar missing from the bundled `pricing.json` may well
  sell the TLD anyway. `eligibility.json` is the same shape of promise in the
  other direction — the restrictions and requirements it lists are sourced, but
  a TLD it does not list is neither unrestricted nor free of obligations.
* Large sweeps get rate-limited. `ds` paces itself per host, backs off on
  403/429, and stops querying a server that has refused it six times in a row
  (retrying it after 30s). Identity Digital runs ~250 gTLDs behind one RDAP
  endpoint with a strict quota, so a single `--tld all` sweep will leave some of
  those unresolved; re-run just those later:

  ```sh
  ds apple --tld "$(sed 's/^apple\.//' unknown.txt | paste -sd, -)"
  ```

