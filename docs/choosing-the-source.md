# Choosing the source

By default a lookup falls through three sources: RDAP, then the bundled
`whois.json` table, then a WHOIS server that IANA says currently serves the TLD.
`--source` restricts that:

| Value | Uses |
| --- | --- |
| `auto` (default) | RDAP -> `whois.json` -> IANA referral |
| `rdap` | RDAP only. TLDs with no RDAP service return `UNKNOWN` immediately, with no network traffic |
| `whois` | `whois.json` only. No RDAP, no IANA referral — exactly what the bundled file says |

```sh
ds apple --tld com,co,de --source rdap     # .co and .de have no RDAP -> UNKNOWN
ds apple --tld com,co,de --source whois    # all three over port 43
```

`--no-iana` is the narrower switch: keep the default order but never talk to
`whois.iana.org`, so a stale bundled WHOIS host is not repaired and `--where`
shows only the registrar links.

## Your own RDAP servers

Point TLDs at servers of your choosing with an `rdap.json`, written in the same
RDAP bootstrap format IANA publishes (RFC 9224) — so you can start from a copy
of `dns.json` and edit it:

```json
{
  "version": "1.0",
  "description": "my RDAP servers",
  "services": [
    [["com"], ["https://rdap.verisign.com/com/v1/"]],
    [["io"], ["https://rdap.identitydigital.services/rdap/"]],
    [["internal"], ["https://rdap.corp.example/", "https://rdap-backup.corp.example/"]]
  ]
}
```

Each entry maps a list of TLDs to a list of servers, tried in order.

```sh
ds apple --tld com,io --rdap-file servers.json              # merged over IANA
ds apple --tld internal --rdap-file servers.json --rdap-mode only
```

| Mode | Effect |
| --- | --- |
| `merge` (default) | your entries win for the TLDs they name, everything else still comes from the IANA bootstrap |
| `only` | the IANA bootstrap is not consulted or downloaded at all |

Without `--rdap-file`, `./rdap.json` is picked up if it exists, then
`~/.config/ds/rdap.json`. A line saying which file was loaded, how many TLDs it
covers and which mode is in force is printed before the results.

This is how you reach a registry that is missing from the bootstrap, test a
staging RDAP server, or serve an internal zone that has no public entry at all.

## Your own WHOIS servers

The same thing for WHOIS, in the same format as the bundled `whois.json` — a
list of extensions, a server and the text that marks a free domain:

```json
[
  { "extensions": ".internal,.corp",
    "uri": "socket://whois.corp.example",
    "available": "not registered" },
  { "extensions": ".de",
    "uri": "socket://whois.denic.de",
    "available": "Status: free" }
]
```

`uri` is `socket://host[:port]` for classic port-43 WHOIS, or an `http(s)://`
prefix the domain is appended to. `available` is matched against the response
to decide the name is free; matching is negation-aware, so a needle of
"Available" is not triggered by "Not Available".

```sh
ds apple --tld de,ch --whois-file servers.json                 # merged
ds apple --tld internal --whois-file servers.json --whois-mode only
```

| Mode | Effect |
| --- | --- |
| `merge` (default) | your entries win for the TLDs they name, the rest of the bundled table still applies |
| `only` | the bundled table is ignored entirely |

`./whois.json` and `~/.config/ds/whois.json` are picked up automatically, and
the run says which file it loaded — exactly as for `rdap.json`.

## Your own prices

And the same again for the price column, in the format of the bundled
`pricing.json` — a TLD mapped to what each registrar charges for it:

```json
{
  "com": [
    { "register": "porkbun.com",  "prices": { "regular": 9.13, "renew": 11.06 } },
    { "register": "namesilo.com", "prices": { "regular": 9.95, "renew": 11.79 } }
  ],
  "internal": [
    { "register": "corp.example", "prices": { "regular": 0.0 } }
  ]
}
```

Where several registrars quote a TLD the column shows the mean, so `.com` above
reads `$9.54`. Each price may be left out or set to `null` for "not sold", and a
TLD that ends up with no registration price at all shows `-`.

`register` names the registrar, and `--where` reads it: a quote here is what
tells `ds` that this registrar sells that TLD, so the two registrars above are
what `ds apple --tld com --where` would then offer you. Leave it out and the
price still counts toward the column, but the entry claims nothing about where
to buy the name.

```sh
ds apple --tld com,io --pricing-file myprices.json                 # merged
ds apple --tld internal --pricing-file myprices.json --pricing-mode only
```

| Mode | Effect |
| --- | --- |
| `merge` (default) | your prices win for the TLDs they name, the rest of the bundled table still applies |
| `only` | the bundled table is ignored entirely; TLDs you did not price show `-` |

`./pricing.json` and `~/.config/ds/pricing.json` are picked up automatically,
and the run says which file it loaded — exactly as for `rdap.json`.

This is how you put your registrar's real prices in the column, price a TLD the
bundled table has never heard of, or hold a corporate zone at nothing.

