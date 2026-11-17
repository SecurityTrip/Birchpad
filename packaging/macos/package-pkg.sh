#!/usr/bin/env bash
# Build the installer and its matching full update package.
set -euo pipefail
shopt -s nullglob

if [[ $# != 4 || ! -x "${1:-}/Contents/MacOS/birchpad" || ! -f "${1:-}/Contents/Info.plist" ]]; then
    echo "Usage: $0 path/to/Birchpad.app version channel output-directory" >&2
    exit 2
fi
app=$1
version=$2
channel=$3
output=$4
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Both the prebuilt app and Velopack's UpdateMac support Apple silicon and Intel.
vpk pack --packId Birchpad --packVersion "$version" --packTitle Birchpad \
    --packAuthors "The Birchpad Contributors" --packDir "$app" \
    --mainExe birchpad --runtime osx --channel "macos-$channel" \
    --delta None --noPortable --outputDir "$work"

installers=("$work"/*.pkg)
packages=("$work"/*-full.nupkg)
if [[ ${#installers[@]} != 1 || ${#packages[@]} != 1 ||
      ! -s "${installers[0]:-}" || ! -s "${packages[0]:-}" ]]; then
    echo "Expected one macOS installer and one full Velopack package" >&2
    exit 1
fi
mkdir -p "$output"
name="birchpad-$version-macos"
cp "${installers[0]}" "$output/$name.pkg"
cp "${packages[0]}" "$output/$name-full.nupkg"
