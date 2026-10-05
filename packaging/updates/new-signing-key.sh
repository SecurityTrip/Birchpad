#!/usr/bin/env bash
# Creates the ed25519 key that signs Birchpad's update manifest (ADR 0020) and stores it in the
# repository's settings; see new-signing-key.ps1 and README.md next to this script. Usage:
#   packaging/updates/new-signing-key.sh [owner/name]
set -euo pipefail
repo="${1:-SecurityTrip/Birchpad}"
environment="update-signing"
cd "$(dirname "$0")/../.."

for tool in cargo gh; do
  command -v "$tool" > /dev/null || { echo "$tool is not installed" >&2; exit 1; }
done
gh auth status > /dev/null 2>&1 || { echo "sign in first: gh auth login" >&2; exit 1; }
if existing="$(gh variable get BIRCHPAD_TRUSTED_KEYS --repo "$repo" 2> /dev/null)" && [ -n "${existing// /}" ]; then
  echo "BIRCHPAD_TRUSTED_KEYS is set already: to replace the key, see packaging/updates/README.md" >&2
  exit 1
fi

cargo build --release --locked -p birchpad-release-tool
tool=target/release/birchpad-release
secret="$("$tool" keygen 2> /dev/null)"
public="$(BIRCHPAD_UPDATE_KEY="$secret" "$tool" public-key)"

printf '\nThe secret key (keep it in a password manager or offline; it is not saved anywhere):\n\n    %s\n\n' "$secret"
read -r -p "Press Enter once it is stored, to put it into the repository's settings and clear the screen "
clear

gh api --method PUT "repos/$repo/environments/$environment" --silent
printf '%s' "$secret" | gh secret set BIRCHPAD_UPDATE_KEY --repo "$repo" --env "$environment"
unset secret
gh variable set BIRCHPAD_TRUSTED_KEYS --repo "$repo" --body "$public"

echo "BIRCHPAD_UPDATE_KEY is set in the $environment environment of $repo."
echo "BIRCHPAD_TRUSTED_KEYS is $public; builds from now on trust it."
