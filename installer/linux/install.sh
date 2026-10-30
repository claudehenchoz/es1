#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Claude Henchoz

# Install ES 1 for the current user (~/.vst3), or with --system for all users (/usr/lib/vst3,
# run with sudo). --uninstall removes it again from the same place.

set -eu

usage() {
    echo "usage: $0 [--system] [--uninstall]"
    echo "  (default)     install to ~/.vst3 for the current user"
    echo "  --system      install to /usr/lib/vst3 for all users (needs sudo)"
    echo "  --uninstall   remove ES 1 instead"
}

here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
dest="$HOME/.vst3"
action=install
for arg in "$@"; do
    case "$arg" in
        --system) dest=/usr/lib/vst3 ;;
        --uninstall) action=uninstall ;;
        -h | --help) usage; exit 0 ;;
        *) usage >&2; exit 2 ;;
    esac
done

target="$dest/ES 1.vst3"
if [ "$action" = uninstall ]; then
    rm -rf "$target"
    echo "Removed $target"
    exit 0
fi

mkdir -p "$dest"
rm -rf "$target"
cp -R "$here/ES 1.vst3" "$dest/"
echo "Installed $target"
echo "Restart your host or rescan plugins to find ES 1."
