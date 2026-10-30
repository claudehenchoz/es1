#!/bin/sh
# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Claude Henchoz

# Build the macOS installer package: installs "ES 1.vst3" into /Library/Audio/Plug-Ins/VST3.
#
#   installer/macos/build-pkg.sh <version> <path to ES 1.vst3> <output .pkg>

set -eu

version=$1
bundle=$2
out=$3
id=com.nih-plug.es1-vst3.pkg
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/root" "$work/resources" "$work/packages"
cp -R "$bundle" "$work/root/"

# Install exactly where we say: without this, Installer "relocates" a bundle to wherever a copy
# with the same identifier already exists on the disk.
pkgbuild --analyze --root "$work/root" "$work/components.plist"
plutil -replace 0.BundleIsRelocatable -bool NO "$work/components.plist"

pkgbuild --root "$work/root" --component-plist "$work/components.plist" \
    --identifier "$id" --version "$version" \
    --install-location /Library/Audio/Plug-Ins/VST3 \
    "$work/packages/es1.pkg"

cp "$here/welcome.txt" "$work/resources/welcome.txt"
sed -e "s/@VERSION@/$version/g" -e "s/@ID@/$id/g" "$here/distribution.xml" > "$work/distribution.xml"
productbuild --distribution "$work/distribution.xml" --resources "$work/resources" \
    --package-path "$work/packages" "$out"
echo "Built $out"
