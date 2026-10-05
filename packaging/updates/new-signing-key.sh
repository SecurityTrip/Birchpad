#!/usr/bin/env bash
# Creates the ed25519 key that signs Birchpad's update manifest (ADR 0020); see
# new-signing-key.ps1 and README.md next to this script. Usage:
#   packaging/updates/new-signing-key.sh [owner/name]
set -euo pipefail
repo="${1:-SecurityTrip/Birchpad}"
environment="update-signing"
cd "$(dirname "$0")/../.."

for tool in cargo gh; do
  command -v "$tool" > /dev/null || { echo "$tool is not installed" >&2; exit 1; }
done
gh auth status > /dev/null || { echo "sign in first: gh auth login" >&2; exit 1; }

cargo build --release --locked -p birchpad-release-tool
secret="$(target/release/birchpad-release keygen)"

printf '\nThe secret key (keep it in a password manager or offline; it is not saved anywhere):\n\n    %s\n\n' "$secret"
read -r -p "Press Enter once it is stored, to put it into the repository secret and clear the screen "
clear

gh api --method PUT "repos/$repo/environments/$environment" --silent
printf '%s' "$secret" | gh secret set BIRCHPAD_UPDATE_KEY --repo "$repo" --env "$environment"
unset secret

echo "BIRCHPAD_UPDATE_KEY is set in the $environment environment of $repo."
echo "Now commit crates/update/trusted-keys.txt; the next builds trust the new key."
git --no-pager diff -- crates/update/trusted-keys.txt
