# Usage

```sh
ds apple --tld com,net             # a specific list
ds apple --tld all                 # every registrable TLD (~1330 lookups)
ds apple --tld popular             # a curated set of ~38 common TLDs
ds apple --tld rdap                # only TLDs that have an RDAP service
ds apple --tld @tlds.txt           # one TLD per line, `,` and `#` comments ok
ds apple.com                       # a full domain, checked as-is
```

Results stream in as they arrive. `+` is available, `-` is taken, `!` is a TLD
you cannot register in, `?` could not be answered:

```
+ mybrand.dev                      AVAILABLE    $14.74 rdap      415ms
- apple.com                        TAKEN        $13.68 rdap      978ms
! mybrand.aws                      PRIVATE           - rdap      312ms  .aws is a brand TLD — only AWS Registry LLC registers names there (ICANN Spec 13)
? google.pt                        UNKNOWN           - -        3548ms  whois: connecting to whois.dns.pt:43: timed out

summary: 1 available  1 taken  1 private  1 unknown   (4 checked in 3.5s)
```

The fourth column is the average first-year registration price for the TLD; `-`
means no registrar in the bundled table prices it.

The exit code is `0` if anything is available, `1` if nothing is, `2` on a
startup error — so `ds mybrand --tld com -q && echo free` works in a script.

## Several names at once

Comma separated, as separate arguments, or from a file. Every name is checked
against every TLD:

```sh
ds apple,orange,bangla,english --tld com,net    # 4 names x 2 TLDs = 8 lookups
ds apple orange bangla --tld io
ds @names.txt --tld com,net,io --available-only
```

`names.txt` takes one name per line; commas work there too, `#` starts a
comment, blank lines are ignored and duplicates are dropped:

```
apple
orange, bangla     # both checked
english
```

## Saving the results

Nothing is written unless you ask for it:

```sh
ds mybrand --tld all --save                 # available.txt, unavailable.txt
ds mybrand --tld all -o results             # a directory implies --save
ds mybrand --tld popular --append           # so does --append
```

* `available.txt` — one domain per line
* `unavailable.txt` — registered domains
* `private.txt` — only when a [private TLD](choosing-tlds.md#brand-and-reserved-tlds) was
  checked: names that are unregistered but not for sale
* `unknown.txt` — only when a registry could not be reached, so a failed lookup
  is never filed as "available"

With `--json` the same files are written as `available.json`,
`unavailable.json`, `private.json` and `unknown.json`, each a JSON array of the
full results rather than a list of names:

```sh
ds mybrand --tld all --save --json          # available.json, unavailable.json
```

`--append` merges into the array already in the file, so the result is still
one valid JSON document.

`--available-only` trims what is printed, not what is saved.

