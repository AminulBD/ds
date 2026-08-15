#!/bin/sh
# Copies ds and its manual page out of this disk image into /usr/local.
set -e

here=$(cd "$(dirname "$0")" && pwd)
bin=${BIN_DIR:-/usr/local/bin}
man=${MAN_DIR:-/usr/local/share/man/man1}

echo "Installing ds to $bin (you may be asked for your password)"
sudo install -d "$bin" "$man"
sudo install -m755 "$here/ds" "$bin/ds"
sudo install -m644 "$here/ds.1" "$man/ds.1"

echo "Done: $("$bin/ds" --version), manual at 'man ds'"
