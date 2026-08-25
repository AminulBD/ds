# Install

On macOS or Linux with [Homebrew](https://brew.sh):

```sh
brew install aminulbd/tap/ds
```

That is the one to take if you have it — upgrades come with `brew upgrade`.
Without Homebrew, this installs the latest release into `~/.local/bin`, needing
no root and nothing preinstalled but `curl` and `tar`:

```sh
curl -fsSL https://raw.githubusercontent.com/aminulbd/ds/main/packaging/install.sh | sh
```

It creates the directories it needs, and says so if they turn out not to be on
your `PATH` or on the manual search path. Set `BIN_DIR` to install elsewhere,
or run it as root to install system-wide. It is worth reading before you pipe
it to a shell — drop the `| sh` and it just prints.

Every release also ships installers and plain archives for Linux, macOS and
Windows on x86_64, arm64 and 32-bit x86. Grab one from the
[releases page](https://github.com/AminulBD/ds/releases):

| Platform | File | Install |
| --- | --- | --- |
| Debian, Ubuntu | `ds_<version>_amd64.deb` (also `arm64`, `i386`) | `sudo dpkg -i ds_*.deb` |
| Fedora, RHEL, openSUSE | `ds-<version>.x86_64.rpm` (also `aarch64`, `i686`) | `sudo rpm -i ds-*.rpm` |
| macOS | `ds-<version>-aarch64-apple-darwin.dmg` (also `x86_64`) | mount it, run `install.sh` |
| Windows | `ds-<version>-x86_64-pc-windows-msvc.msi` (also `aarch64`, `i686`) | double-click, then open a new terminal — see [Windows](#windows) |
| Anything else | `.tar.gz` / `.zip` | unpack and copy `ds` onto your `PATH` |

The `.deb` and `.rpm` carry the binary, the man page and the licence, and are
built from static musl binaries — they depend on nothing.

## Windows

Take `x86_64` unless the machine is an Arm one — a Snapdragon laptop or a
Surface Pro X — which wants `aarch64`. `i686` is there for 32-bit Windows only.

`ds.exe` is built against the Microsoft C runtime, so it needs the Visual C++
Redistributable — most machines already have it, and the giveaway when they do
not is `VCRUNTIME140.dll was not found` on the first run. Install the one that
matches the build you took:

| Build | Download |
| --- | --- |
| `x86_64` | [vc_redist.x64.exe](https://aka.ms/vs/17/release/vc_redist.x64.exe) |
| `aarch64` | [vc_redist.arm64.exe](https://aka.ms/vs/17/release/vc_redist.arm64.exe) |
| `i686` | [vc_redist.x86.exe](https://aka.ms/vs/17/release/vc_redist.x86.exe) |

Those are Microsoft's permanent links to the current release; the page behind
them is [Latest supported Visual C++ Redistributable downloads](https://learn.microsoft.com/cpp/windows/latest-supported-vc-redist).

The `.msi` installs either for everyone or for you alone. Left alone it takes
the first route when it can: Windows asks for administrator rights and `ds.exe`
lands in `C:\Program Files\ds\bin`. **Advanced** on the first screen offers the
choice — installing for the current user only wants no administrator and goes to
`%LOCALAPPDATA%\Apps\ds\bin` instead. Either way the folder is appended to
`PATH`, the system one or yours, and a terminal that was already open keeps its
old `PATH`, so open a new one before typing `ds`.

The installer is not code signed, so SmartScreen greets it with "Windows
protected your PC". **More info** then **Run anyway** gets past it; if you would
rather check first, every release ships a `SHA256SUMS` file:

```powershell
Get-FileHash .\ds-<version>-x86_64-pc-windows-msvc.msi -Algorithm SHA256
```

Uninstalling is the usual Settings → Apps route, and both halves can be done
without the UI:

```powershell
msiexec /i ds-<version>-x86_64-pc-windows-msvc.msi /qn                # silent
msiexec /i ds-<version>-x86_64-pc-windows-msvc.msi /qn ALLUSERS=1     # everyone
msiexec /i ds-<version>-x86_64-pc-windows-msvc.msi /qn ALLUSERS=""    # just me
msiexec /x ds-<version>-x86_64-pc-windows-msvc.msi /qn                # remove
```

Without `ALLUSERS` it installs for everyone from an elevated prompt and for the
current user from an ordinary one.

Without administrator rights, take the `.zip` instead: unpack it and put
`ds.exe` wherever you like — the binary needs nothing beside it.

Colours work in Windows Terminal, PowerShell and `cmd.exe` from Windows 10 1809
onwards. An older console gets plain text rather than escape codes, and
`--no-color` or `NO_COLOR` turns them off anywhere.

Or build it yourself:

```sh
cargo build --release
install -m755 target/release/ds ~/.local/bin/ds
install -m644 ds.1 ~/.local/share/man/man1/ds.1     # then: man ds
```

`whois.json`, `pricing.json`, `private-tlds.json` and `eligibility.json` are
embedded at compile
time, so the binary runs from anywhere. `tld-facts.json` and
`tld-categories.json` — the A–Z of every TLD, what kind it is, what it is for
and who runs it — ship beside them but are not embedded; see
[The A–Z of TLDs](the-az-of-tlds.md).

