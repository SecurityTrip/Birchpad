# Creates the ed25519 key that signs Birchpad's update manifest (ADR 0020):
# - its public half is added to crates/update/trusted-keys.txt, to commit;
# - its secret is shown once, to keep offline, and stored as the BIRCHPAD_UPDATE_KEY secret of
#   the update-signing environment of the repository, through the GitHub CLI. It is never
#   written to disk.
#
# Needs: Rust (cargo), the GitHub CLI (gh) signed in with admin rights on the repository.
# Run from anywhere:  pwsh packaging/updates/new-signing-key.ps1 [-Repo owner/name]
param(
    [string]$Repo = "SecurityTrip/Birchpad",
    [string]$Environment = "update-signing"
)
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..\..")

foreach ($tool in "cargo", "gh") {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool is not installed" }
}
gh auth status | Out-Null
if ($LASTEXITCODE -ne 0) { throw "sign in first: gh auth login" }

cargo build --release --locked -p birchpad-release-tool
if ($LASTEXITCODE -ne 0) { throw "cannot build the release tool" }
$secret = (& "target\release\birchpad-release" keygen | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or -not $secret) { throw "cannot create the key" }

Write-Host ""
Write-Host "The secret key (keep it in a password manager or offline; it is not saved anywhere):" -ForegroundColor Yellow
Write-Host ""
Write-Host "    $secret"
Write-Host ""
Read-Host "Press Enter once it is stored, to put it into the repository secret and clear the screen"
Clear-Host

# The environment can then get protection rules (required reviewers, allowed branches) in the
# repository settings.
gh api --method PUT "repos/$Repo/environments/$Environment" --silent
if ($LASTEXITCODE -ne 0) { throw "cannot create the $Environment environment" }
$secret | gh secret set BIRCHPAD_UPDATE_KEY --repo $Repo --env $Environment
if ($LASTEXITCODE -ne 0) { throw "cannot set the secret" }
Remove-Variable secret

Write-Host "BIRCHPAD_UPDATE_KEY is set in the $Environment environment of $Repo."
Write-Host "Now commit crates/update/trusted-keys.txt; the next builds trust the new key."
git --no-pager diff -- crates/update/trusted-keys.txt
