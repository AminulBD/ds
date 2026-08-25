# Choosing TLDs

## By level

`--level` filters the `--tld` list by where the name would actually sit:

| Value | Keeps | Count under `--tld all` |
| --- | --- | --- |
| `any` (default) | everything | 1329 |
| `second` | plain TLDs — `apple.com`, `apple.de` | 944 |
| `third` | multi-label suffixes — `apple.co.uk`, `apple.com.au` | 385 |

Handy for `--tld all`, which otherwise sweeps hundreds of restricted zones
(`gov.bd`, `ernet.in`, `edu.gt`) you cannot register under anyway. A full domain
typed out by hand is always checked as given — `--level` only filters TLD lists.

## By length

`--cctld` keeps only country-code TLDs — the two-letter ones, which is exactly
what ICANN reserves for ISO 3166-1 country codes:

```sh
ds apple --tld all --cctld                  # 538 (includes co.uk, com.au, ...)
ds apple --tld all --cctld --level second   # 173 bare ccTLDs: de, io, jp, ...
```

`--tld-len` is the general form, measured on the last label so `co.uk` counts
as 2:

| Spec | Keeps | Count under `--tld all` |
| --- | --- | --- |
| `--tld-len 2` | two-letter (same as `--cctld`) | 538 |
| `--tld-len 3` | `com`, `net`, `xyz`, ... | 157 |
| `--tld-len -3` | three characters or fewer | 695 |
| `--tld-len 4-` | four or more | 634 |

## Brand and reserved TLDs

Some TLDs are not on sale to anybody. `.aws`, `.google`, `.bmw` and 364 others
are **brand TLDs**: the registry operator is the only party that may hold a
name in the zone, so a lookup for `mybrand.aws` truthfully answers "no such
domain" — which reads as AVAILABLE and is worth nothing. `.arpa` and the names
the IETF reserves (`.test`, `.example`, `.invalid`, `.localhost`, `.local`,
`.onion`) are closed for the same practical reason.

`ds` reports those as `PRIVATE` rather than `AVAILABLE`, with the reason and
the operator attached:

```sh
ds mybrand --tld aws
```

```
! mybrand.aws                      PRIVATE           - rdap      312ms  .aws is a brand TLD — only AWS Registry LLC registers names there (ICANN Spec 13)
```

**They are left out of `--tld all`, `--tld rdap` and `--tld popular` by
default** — 368 of the 1697 TLDs in `all`, so a sweep is a fifth shorter and
carries no dead ends. The run says so when it happens. A TLD you name yourself
is always checked; the default only prunes the lists `ds` picks for you.

| `--private` | Effect |
| --- | --- |
| *(unset)* | excluded from `all` / `rdap` / `popular`; a TLD you name is still checked |
| `exclude` | excluded everywhere, a hand-written `--tld` list included |
| `include` | checked, and still reported `PRIVATE` |
| `only` | check nothing else — what a sweep is skipping |

```sh
ds mybrand --tld all --private include      # all 1697, brands marked PRIVATE
ds mybrand --tld all --private only         # just the 368 closed ones
```

The classification is bundled as `private-tlds.json` and comes from ICANN's
machine-readable [registry-agreement
list](https://www.icann.org/resources/registries/gtlds/v2/gtlds.json). A
registry that runs its TLD purely for itself applies for **Specification 13**,
the ".brand TLD" exemption from the registrar non-discrimination rules, and
that flag is what `ds` reads — a dated, first-party, checkable fact rather than
a hand-picked list of famous names. Regenerate it with `node
scripts/build-private-tlds.mjs`.

The converse is deliberately *not* asserted. A TLD missing from that file is
not thereby claimed to be registrable; `ds` simply makes no claim and reports
whatever the registry said. So `.app` and `.dev` — Google-run, but with no
Specification 13 and sold through ordinary registrars — are ordinary TLDs here,
and every ccTLD is left exactly as it was.

