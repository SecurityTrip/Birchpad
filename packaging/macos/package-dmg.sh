#!/usr/bin/env bash
# Package a Birchpad app bundle with an Applications shortcut.
set -euo pipefail

if [[ $# != 2 || ! -x "${1:-}/Contents/MacOS/birchpad" || ! -f "${1:-}/Contents/Info.plist" ]]; then
    echo "Usage: $0 path/to/Birchpad.app path/to/output.dmg" >&2
    exit 2
fi
app=$1
output=$2
work=$(mktemp -d)
image_mount="$work/mount"
mounted=false
cleanup() {
    if "$mounted"; then
        hdiutil detach "$image_mount" || return
    fi
    rm -rf "$work"
}
trap cleanup EXIT

mkdir "$work/stage" "$image_mount"
ditto "$app" "$work/stage/Birchpad.app"
ln -s /Applications "$work/stage/Applications"
codesign --force --deep --sign - "$work/stage/Birchpad.app"
hdiutil create -volname Birchpad -srcfolder "$work/stage" -fs HFS+ -format UDZO "$output"
# Check the image that users receive, including the symlink and the copied signature.
hdiutil attach -readonly -nobrowse -mountpoint "$image_mount" "$output"
mounted=true
test "$(readlink "$image_mount/Applications")" = /Applications
codesign --verify --deep --strict "$image_mount/Birchpad.app"
