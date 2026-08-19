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
mkdir -p "$BIN_DIR"
cp "$EXTRACT_DIR/ds" "$BIN_DIR/ds"
chmod 755 "$BIN_DIR/ds"

if [ -f "$EXTRACT_DIR/ds.1" ]; then
  mkdir -p "$MAN_DIR" 2>/dev/null || true
  if [ -d "$MAN_DIR" ]; then
    cp "$EXTRACT_DIR/ds.1" "$MAN_DIR/ds.1" 2>/dev/null || true
    chmod 644 "$MAN_DIR/ds.1" 2>/dev/null || true
  fi
fi

echo ""
echo " Successfully installed ds to $BIN_DIR/ds"

# 9. Verify PATH
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *)
    echo ""
    echo "Note: '$BIN_DIR' is not in your current PATH."
    echo "To run 'ds' directly from your shell, add this to your ~/.bashrc or ~/.zshrc:"
    echo "    export PATH=\"$BIN_DIR:\$PATH\""
    ;;
esac
