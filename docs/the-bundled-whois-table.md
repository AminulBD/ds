# The bundled WHOIS table

Most of `whois.json` is generated rather than hand-maintained, by
[`scripts/refresh-whois.py`](../scripts/refresh-whois.py). It walks IANA's list of
delegated TLDs, asks `whois.iana.org` which server serves each one, and then —
the part that matters — tests every answer before believing it. For each TLD it
asks the server about a sixteen-character random label nobody can have
registered, and about a name the *DNS* proves is registered because it has NS
records. A server only reaches the table if the first reads as AVAILABLE and the
second reads as TAKEN, judged by a port of `ds`'s own classifier.

The `available` needle is then chosen from a list of registry phrasings vetted by
hand, and kept only if it is absent from the registered name's record. A needle
is never invented by diffing two responses: the wrong needle turns a registered
domain into an `AVAILABLE`, which is the one answer `ds` must never give. A TLD
whose server cannot be shown to tell the two names apart is left out of the file
rather than guessed at, and `ds` falls back to asking IANA at runtime.

`scripts/whois-report.tsv` records the verdict and the reason for every TLD
considered, so a rejection can be looked up rather than wondered about.

```sh
./scripts/refresh-whois.py all        # harvest, verify, rewrite whois.json
cargo build --release
./scripts/refresh-whois.py verify     # re-check the table through ds itself
```

The run is paced to be a good guest: queries to one registry are serialised with
a gap between them, keyed on the address the server resolves to rather than its
name, because hundreds of `whois.nic.<tld>` aliases sit on a handful of shared
back ends. Expect it to take hours.
