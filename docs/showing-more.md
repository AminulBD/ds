# Showing more

| Flag | Shows |
| --- | --- |
| `--details` | registrar, IANA ID, created/updated/expiry dates, status codes, nameservers, DNSSEC, abuse contact |
| `--registry` | which RDAP endpoint and/or WHOIS server answered |
| `--whois` | queries WHOIS as well and prints the raw record |
| `--dns-records` | A, AAAA, NS, MX, TXT, CNAME and SOA records |
| `--where` | for available domains: the registry, its official registration page, any eligibility rule, and the registrars whose published prices show they sell the TLD |
| `--raw` | raw RDAP JSON |
| `--json` | a JSON array instead of the text report |
| `--all-info` | `--details --registry --whois --dns-records --where` |

```sh
ds apple --tld com --details --registry --dns-records
```

```
- apple.com                        TAKEN        $13.68 rdap      978ms
    rdap: https://rdap.verisign.com/com/v1/
    registrar    Nom-iq Ltd. dba COM LAUDE
    created      1987-02-19T05:00:00Z
    expires      2027-02-20T05:00:00Z
    iana id      470
    status       client delete prohibited, client transfer prohibited, ...
    nameservers  a.ns.apple.com, b.ns.apple.com, c.ns.apple.com, d.ns.apple.com
    dnssec       unsigned
    abuse        abuse@comlaude.com
    a            17.253.144.10
    mx           10 mx-in.g.apple.com., 20 mx-in-hfd.apple.com., ...
```

## Where to register

```sh
ds mynewbrand --tld de,fr --where
```

```
+ mynewbrand.de                    AVAILABLE     $6.29 whois     858ms
    registry     DENIC eG
    registry url http://www.denic.de/
    register at  porkbun.com       $2.90  https://porkbun.com/checkout/search?q=mynewbrand.de
                 namecheap.com     $6.98  https://www.namecheap.com/domains/registration/results/?domain=mynewbrand.de
+ mynewbrand.fr                    AVAILABLE    $17.98 rdap     1179ms
    registry     Association Française pour le Nommage Internet en Coopération (A.F.N.I.C.)
    registry url https://www.nic.fr
    eligibility  EU presence: the registrant must reside in, or have its registered office in, the EU/EEA or Switzerland
                 https://www.afnic.fr/en/observatory-and-resources/documents-to-consult-or-download/naming-policies/
    register at  namecheap.com    $17.98  https://www.namecheap.com/domains/registration/results/?domain=mynewbrand.fr
```

The registry name and URL come from IANA's record for the TLD, looked up once
per TLD and shown only for domains that are actually available. For ccTLDs that
page is usually the registry's own list of accredited registrars, which is the
answer worth having when a TLD is not sold on the open market.

**`register at` is evidence, not a guess.** Each line is a registrar that
publishes a price for that TLD in [`pricing.json`](choosing-the-source.md#your-own-prices), cheapest
first — nobody quotes a price for something they cannot sell you. That is why
`.fr` above lists one registrar and `.de` two: Porkbun's price list carries
`.de` and not `.fr`. A TLD no registrar in the table prices says so plainly:

```
+ mynewbrand.edu                   AVAILABLE         - whois     365ms
    eligibility  US institutions accredited by an agency the Department of Education recognises
                 https://www.educause.edu/edu-domain
    register at  no registrar in the price table sells .edu
```

**`eligibility` is who may buy it.** A name being free says nothing about
whether the registry would let you have it, so a TLD with a residency, nexus or
membership rule prints that rule and the registry page it came from *before* the
places to buy it. This comes from the bundled `eligibility.json`, which — unlike
`whois.json` and `pricing.json` — is hand-maintained, because no registry
publishes eligibility rules in a machine-readable form. Every entry names the
page it was taken from, and that page, not `ds`, is the authority. The list
covers the restricted TLDs a registrar is likely to offer you; **a TLD missing
from it is not thereby open**.

Both halves are bundled, so `--where` still answers the useful part of the
question with `--no-iana` or no route to `whois.iana.org` — only the registry
name and URL drop out.

