<!--
Thanks for sending a change. CONTRIBUTING.md has the details:
https://github.com/AminulBD/ds/blob/main/CONTRIBUTING.md
-->

## What this changes, and why

<!-- The reason is the part a reviewer cannot reconstruct from the diff. -->

Fixes #

## How it was verified

<!--
The commands you ran, and what they printed — before and after, for a bug fix.
CI does not run the tests, so please say which of these you ran:

    cargo test
    cargo clippy --all-targets
    cargo fmt
    cargo test --features serve            # if you touched src/serve.rs
    python3 scripts/test_whois_classify.py # if you touched the WHOIS classifier
    node scripts/test-tld-facts-parse.mjs  # if you touched the TLD facts or categories
-->

## Checklist

- [ ] Anything I could not verify — a registry I could not reach, a platform I
      could not test on — is called out above.
- [ ] A new or changed flag is in `--help`, in the matching page under `docs/`,
      and in `ds.1`.
- [ ] A generated data file (`whois.json`, `pricing.json`, `private-tlds.json`,
      `tld-facts.json`) was rebuilt with its script rather than hand-edited, and
      I said which script and when.
- [ ] A hand-maintained entry (`eligibility.json`, `tld-categories.json`) names
      the page it came from.
- [ ] No lookup can now report `AVAILABLE` on anything less than the registry's
      own answer.
