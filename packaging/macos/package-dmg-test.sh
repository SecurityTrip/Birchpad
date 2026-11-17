#!/usr/bin/env bash
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
app="$work/path with spaces/Birchpad.app"
image="$work/Birchpad test.dmg"

# The smallest bundle the packager can sign and put in an image.
mkdir -p "$app/Contents/MacOS"
sed 's/@VERSION@/0.0.1/g' "$here/Info.plist" > "$app/Contents/Info.plist"
if bash "$here/package-dmg.sh" "$app" "$image"; then
    echo "Packaging accepted an incomplete bundle" >&2
    exit 1
fi
test ! -e "$image"
printf 'int main(void) { return 0; }\n' | clang -x c - -o "$app/Contents/MacOS/birchpad"
bash "$here/package-dmg.sh" "$app" "$image"
hdiutil verify "$image"
