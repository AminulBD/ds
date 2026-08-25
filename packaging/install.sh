#!/bin/sh
# One-line installer for ds (domain search)
# Usage: curl -fsSL https://raw.githubusercontent.com/aminulbd/ds/main/packaging/install.sh | sh
set -eu

REPO="aminulbd/ds"
BIN_NAME="ds"

# Allow override via environment variables
BIN_DIR="${BIN_DIR:-}"
MAN_DIR="${MAN_DIR:-}"
VERSION="${VERSION:-}"

# 1. Determine OS
OS="$(uname -s)"
case "$OS" in
  Linux)
    OS_TYPE="unknown-linux-musl"
    ;;
  Darwin)
    OS_TYPE="apple-darwin"
    ;;
  *)
    echo "Error: unsupported operating system: $OS" >&2
    echo "ds supports Linux and macOS. For Windows, download the installer from https://github.com/$REPO/releases" >&2
    exit 1
    ;;
esac

# 2. Determine Architecture
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64|amd64)
    ARCH_TYPE="x86_64"
    ;;
  aarch64|arm64)
    ARCH_TYPE="aarch64"
    ;;
  i386|i686)
    if [ "$OS" = "Darwin" ]; then
      echo "Error: 32-bit macOS is not supported." >&2
      exit 1
    fi
    ARCH_TYPE="i686"
    ;;
  *)
    echo "Error: unsupported CPU architecture: $ARCH" >&2
    exit 1
    ;;
esac

TARGET="${ARCH_TYPE}-${OS_TYPE}"

# 3. Determine install destinations if not set
if [ -z "$BIN_DIR" ]; then
  if [ "$(id -u)" -eq 0 ]; then
    BIN_DIR="/usr/local/bin"
    MAN_DIR="${MAN_DIR:-/usr/local/share/man/man1}"
  else
    BIN_DIR="$HOME/.local/bin"
    MAN_DIR="${MAN_DIR:-$HOME/.local/share/man/man1}"
  fi
elif [ -z "$MAN_DIR" ]; then
  MAN_DIR="$(dirname "$BIN_DIR")/share/man/man1"
fi

# 4. Resolve latest version if not provided
if [ -z "$VERSION" ]; then
  echo "Finding latest release of $REPO..."
  if command -v curl >/dev/null 2>&1; then
    LATEST_URL="$(curl -fsSLI -o /dev/null -w "%{url_effective}" "https://github.com/$REPO/releases/latest" 2>/dev/null || true)"
    VERSION="${LATEST_URL##*/}"
  fi

  # Fallback to GitHub API if redirect failed or returned empty
  if [ -z "$VERSION" ] || [ "$VERSION" = "latest" ]; then
    if command -v curl >/dev/null 2>&1; then
      VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || true)"
    elif command -v wget >/dev/null 2>&1; then
      VERSION="$(wget -qO- "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || true)"
    fi
  fi

  if [ -z "$VERSION" ] || [ "$VERSION" = "latest" ]; then
    echo "Error: unable to determine latest release version." >&2
    exit 1
  fi
fi

# Ensure VERSION starts with 'v'
case "$VERSION" in
  v*) ;;
  *) VERSION="v$VERSION" ;;
esac

TARBALL="ds-${VERSION}-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/${VERSION}/${TARBALL}"

echo "Installing ds $VERSION ($TARGET)..."

# 5. Create temporary directory for download
TMPDIR="$(mktemp -d 2>/dev/null || mktemp -d -t ds-install)"
cleanup() {
  rm -rf "$TMPDIR"
}
trap cleanup EXIT INT TERM

# 6. Download archive
echo "Downloading $DOWNLOAD_URL..."
if command -v curl >/dev/null 2>&1; then
  curl -fSL "$DOWNLOAD_URL" -o "$TMPDIR/$TARBALL"
elif command -v wget >/dev/null 2>&1; then
  wget -qO "$TMPDIR/$TARBALL" "$DOWNLOAD_URL"
else
  echo "Error: neither curl nor wget was found on your system." >&2
  exit 1
fi

# 7. Extract archive
tar -xzf "$TMPDIR/$TARBALL" -C "$TMPDIR"

EXTRACT_DIR="$TMPDIR/ds-${VERSION}-${TARGET}"
if [ ! -d "$EXTRACT_DIR" ]; then
  # Fallback in case archive structure changes
  EXTRACT_DIR="$(find "$TMPDIR" -mindepth 1 -maxdepth 1 -type d | grep -v '^tmp' | head -n 1)"
fi

if [ ! -f "$EXTRACT_DIR/ds" ]; then
  echo "Error: extracted archive does not contain the 'ds' executable." >&2
  exit 1
fi

# 8. Install files
#
# Neither ~/.local/bin nor ~/.local/share/man/man1 is guaranteed to exist on a
# fresh system, so both are created here. A failure to create the binary's
# directory is fatal -- there is nowhere to put ds -- while a failure on the
# man directory only costs the manual page, so it is reported and stepped over
# rather than aborting an otherwise good install.
if ! mkdir -p "$BIN_DIR" 2>/dev/null; then
  echo "Error: could not create '$BIN_DIR'." >&2
  echo "Pick a different location with BIN_DIR=/some/where, or create it yourself first." >&2
  exit 1
fi

cp "$EXTRACT_DIR/ds" "$BIN_DIR/ds"
chmod 755 "$BIN_DIR/ds"

MAN_INSTALLED=0
if [ -f "$EXTRACT_DIR/ds.1" ]; then
  if mkdir -p "$MAN_DIR" 2>/dev/null && cp "$EXTRACT_DIR/ds.1" "$MAN_DIR/ds.1" 2>/dev/null; then
    chmod 644 "$MAN_DIR/ds.1" 2>/dev/null || true
    MAN_INSTALLED=1
  else
    echo ""
    echo "Note: could not install the manual page to '$MAN_DIR'." >&2
    echo "      ds itself is installed and works; only 'man ds' is unavailable." >&2
  fi
fi

echo ""
echo "Successfully installed ds to $BIN_DIR/ds"
if [ "$MAN_INSTALLED" -eq 1 ]; then
  echo "Installed the manual page to $MAN_DIR/ds.1"
fi

# 9. Check the install locations are actually reachable
#
# Installing to a directory the shell never searches leaves a tool that cannot
# be run, and installing a man page where man never looks leaves 'man ds'
# failing -- both silently. Neither location is on a default fresh system, so
# say so plainly and give the exact line to fix it.
NEEDS_PATH=0
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) NEEDS_PATH=1 ;;
esac

# man has its own search path, which is not PATH. Ask man itself where it looks
# when it can tell us, and fall back to MANPATH when it cannot. The directory
# man searches is the parent of man1/, e.g. ~/.local/share/man.
NEEDS_MANPATH=0
MAN_ROOT="$(dirname "$MAN_DIR")"
if [ "$MAN_INSTALLED" -eq 1 ]; then
  MAN_SEARCH=""
  if command -v manpath >/dev/null 2>&1; then
    MAN_SEARCH="$(manpath 2>/dev/null || true)"
  fi
  if [ -z "$MAN_SEARCH" ]; then
    MAN_SEARCH="${MANPATH:-}"
  fi
  case ":$MAN_SEARCH:" in
    *":$MAN_ROOT:"*) ;;
    *) NEEDS_MANPATH=1 ;;
  esac
fi

if [ "$NEEDS_PATH" -eq 1 ] || [ "$NEEDS_MANPATH" -eq 1 ]; then
  # Name the file the user actually has, rather than guessing at both. RC is
  # only ever printed, so the tilde is left unexpanded on purpose -- it reads
  # better than a spelled-out home directory.
  SHELL_KIND="$(basename "${SHELL:-/bin/sh}")"
  # shellcheck disable=SC2088
  case "$SHELL_KIND" in
    zsh)  RC="~/.zshrc" ;;
    bash) RC="~/.bashrc" ;;
    fish) RC="~/.config/fish/config.fish" ;;
    *)    RC="your shell's startup file" ;;
  esac

  echo ""
  if [ "$NEEDS_PATH" -eq 1 ]; then
    echo "Note: '$BIN_DIR' is not on your PATH, so 'ds' will not run by name yet."
  fi
  if [ "$NEEDS_MANPATH" -eq 1 ]; then
    echo "Note: '$MAN_ROOT' is not on your manual search path, so 'man ds' will not find the page yet."
  fi

  echo ""
  echo "Add the following to $RC, then open a new terminal:"
  if [ "$SHELL_KIND" = "fish" ]; then
    [ "$NEEDS_PATH" -eq 1 ] && echo "    fish_add_path $BIN_DIR"
    [ "$NEEDS_MANPATH" -eq 1 ] && echo "    set -gx MANPATH $MAN_ROOT \$MANPATH"
  else
    [ "$NEEDS_PATH" -eq 1 ] && echo "    export PATH=\"$BIN_DIR:\$PATH\""
    [ "$NEEDS_MANPATH" -eq 1 ] && echo "    export MANPATH=\"$MAN_ROOT:\$MANPATH\""
  fi

  if [ "$NEEDS_PATH" -eq 1 ]; then
    echo ""
    echo "Until then, run it by full path:  $BIN_DIR/ds --version"
  fi
fi
