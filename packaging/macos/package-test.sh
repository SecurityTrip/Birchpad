#!/usr/bin/env bash
# Build both macOS downloads from a small fixture bundle.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
app="$work/path with spaces/Birchpad.app"
image="$work/Birchpad test.dmg"

# Velopack writes its metadata into Resources after copying the bundle. Keep an icon there,
# as in the release bundle, so the directory can be copied.
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$here/../icons/Birchpad.icns" "$app/Contents/Resources/"
sed 's/@VERSION@/0.0.1/g' "$here/Info.plist" > "$app/Contents/Info.plist"
if bash "$here/package-dmg.sh" "$app" "$image"; then
    echo "Packaging accepted an incomplete bundle" >&2
    exit 1
fi
test ! -e "$image"
printf 'int main(void) { return 0; }\n' | clang -x c - -o "$app/Contents/MacOS/birchpad"
bash "$here/package-dmg.sh" "$app" "$image"
hdiutil verify "$image"

# A prerelease keeps its full version in the package name and update metadata.
version=0.0.1-nightly.1
output="$work/packages"
dotnet tool install --tool-path "$work/tools" vpk --version "$(cat "$here/../velopack-version")"
export PATH="$work/tools:$PATH"
if bash "$here/package-pkg.sh" "$work/missing.app" "$version" nightly "$output"; then
    echo "Packaging accepted a missing bundle" >&2
    exit 1
fi
test ! -e "$output"
bash "$here/package-pkg.sh" "$app" "$version" nightly "$output"
prefix="$output/birchpad-$version-macos"
pkgutil --expand-full "$prefix.pkg" "$work/pkg"
updater=$(find "$work/pkg" -type f -path '*/Birchpad.app/Contents/MacOS/UpdateMac' -print)
test -x "$updater"
lipo "$updater" -verify_arch arm64 x86_64
unzip -Z1 "$prefix-full.nupkg" > "$work/package-files"
grep -E '(^|/)Contents/MacOS/UpdateMac$' "$work/package-files"
grep -E '(^|/)Contents/Resources/sq\.version$' "$work/package-files"
