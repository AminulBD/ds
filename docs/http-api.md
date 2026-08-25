# HTTP API

`ds` can answer the same checks over HTTP, for a web front end or anything else
that would rather call an API than shell out. It is **not** in the released
binaries or packages: the server is behind a cargo feature, so a default build
stays the dependency-light static CLI it has always been.

```sh
cargo build --release --features serve
ds serve                      # http://127.0.0.1:8080
```

| Endpoint | What it does |
| --- | --- |
| `GET /v1/check?name=…&tld=…` | checks the names and returns the same JSON `ds --json` prints |
| `GET /healthz` | `{"status":"ok","version":"…"}`, never touches a registry |

```console
$ curl -s 'http://127.0.0.1:8080/v1/check?name=apple&tld=com,net'
[
  {"domain":"apple.com","tld":"com","status":"taken","method":"rdap","elapsed_ms":511,
   "price":{"currency":"USD","register":14.98,"renew":18.48,"registrars":1}},
  {"domain":"apple.net","tld":"net","status":"taken","method":"rdap","elapsed_ms":535,
   "price":{"currency":"USD","register":14.98,"renew":18.58,"registrars":1}}
]
```

`name` and `tld` may both be repeated or comma separated, and `tld` takes the
same `popular` and `rdap` keywords `--tld` does. With no `tld`, a dotted name is
a whole domain and a bare label means `.com` — again as on the command line.
Results come back in the order asked for. Anything rejected is a 4xx with
`{"error": "…"}`; `Retry-After` comes with a 429.

`status` is `available`, `taken`, **`unknown`** or **`private`**, and both of the
statuses that are not a plain yes or no mean here exactly what they mean on the
command line. An inconclusive lookup is `unknown`, and a response holding one is
sent `Cache-Control: no-store`, since it is a lookup that failed rather than an
answer to keep. A name in a brand or reserved zone is `private`, with the reason
in `note` — a settled fact out of a bundled table, so it is as cacheable as a
`taken`. Nothing in the server turns either of them into `available`.

There is no `private=` parameter, and the rule from [Brand and reserved
TLDs](choosing-tlds.md#brand-and-reserved-tlds) applies as it does to the CLI: `tld=all`,
`tld=rdap` and `tld=popular` leave the closed zones out, and a TLD the caller
names is checked whatever it is. So `tld=aws` is answered — `private`, with the
operator attached — which is what `--private include` is for on the command
line. What the API will not do is let one request spend hundreds of registry
lookups sweeping zones the bundled table already has the answer for.

## Running one

A reachable `ds` server is an open proxy onto other people's registries: every
request it accepts becomes a query made under *your* address, against a service
that rate-limits hard. It therefore ships closed — loopback only, capped, paced
and throttled — and the defaults are the interesting part:

| Flag | Default | Meaning |
| --- | --- | --- |
| `--host <ADDR>` | `127.0.0.1` | bind address; anything else prints a warning |
| `-p, --port <N>` | 8080 | port |
| `--max-lookups <N>` | 50 | most domains one request may ask about (names × TLDs) |
| `--rate-limit <N>` | 60 | requests per minute per client address; 0 turns it off |
| `--cache-ttl <SECS>` | 300 | how long an answer is reused; 0 turns the cache off |
| `-c, --concurrency <N>` | 20 | lookups in flight across the whole server |
| `--per-host <N>` | 4 | parallel lookups against a single registry server |
| `--timeout <SECS>` | 10 | per-request timeout |
| `--cors <ORIGIN>` | | send `Access-Control-Allow-Origin`, so a browser page may call it |
| `--source <auto\|rdap\|whois>` | `auto` | as the CLI's `--source` |
| `--details` `--registry` `--where` | | as the CLI's flags of the same name |
| `--no-iana` | | never query `whois.iana.org` |

The pacing described under [Pacing](pacing.md) is process-wide, not per request:
one `HostLimiter`, so a hundred clients asking about `.com` queue behind the
same per-host cap and share the same back-off. `--max-lookups` is what rules
out `tld=all` — 50 leaves room for `tld=popular`.

The output flags default off, as they do on the command line. Raw WHOIS and raw
RDAP have no query parameter at all: they are bulky, they are the registry's
text to publish rather than ours to mirror, and WHOIS records carry contact
details the parsed `--details` fields leave out.

Behind a reverse proxy every client looks like the proxy, so `--rate-limit`
becomes one budget for all of them — put the per-user limit in the proxy.
`X-Forwarded-For` is deliberately not trusted: it is a header anyone can write,
and honouring it would hand every client an unlimited supply of identities.

One thing to know about the subcommand: `ds serve` is a name a domain could
have had. In a build with the feature on it starts the server, so checking the
*name* `serve` wants `ds serve.com` or `ds -- serve`. Default builds have no
subcommand and are unaffected.

