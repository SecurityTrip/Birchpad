# Creates the ed25519 key that signs Birchpad's update manifest (ADR 0020), and stores it in the
# repository's settings; nothing goes into the repository's files:
# - its secret is shown once, to keep offline, and stored as the BIRCHPAD_UPDATE_KEY secret of
#   the update-signing environment. It is never written to disk;
# - its public key becomes the BIRCHPAD_TRUSTED_KEYS repository variable, which release builds
#   compile in.
#
# Needs: Rust (cargo), the GitHub CLI (gh) signed in with admin rights on the repository.
# Run from anywhere, in Windows PowerShell or PowerShell 7:
#   powershell -ExecutionPolicy Bypass -File packaging\updates\new-signing-key.ps1 [-Repo owner/name]
param(
    [string]$Repo = "SecurityTrip/Birchpad",
    [string]$Environment = "update-signing"
)
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..\..")

foreach ($tool in "cargo", "gh") {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "$tool is not installed" }
}
# What native commands write to standard error is not an error (Windows PowerShell would stop on
# it): their exit codes are checked.
$ErrorActionPreference = "Continue"
gh auth status *> $null
if ($LASTEXITCODE -ne 0) { throw "sign in first: gh auth login" }
$existing = gh variable get BIRCHPAD_TRUSTED_KEYS --repo $Repo 2> $null
if ($LASTEXITCODE -eq 0 -and "$existing".Trim()) {
    throw "BIRCHPAD_TRUSTED_KEYS is set already: to replace the key, see packaging/updates/README.md"
}

cargo build --release --locked -p birchpad-release-tool
if ($LASTEXITCODE -ne 0) { throw "cannot build the release tool" }
$tool = "target\release\birchpad-release.exe"
$secret = (& $tool keygen 2> $null | Out-String).Trim()
if ($LASTEXITCODE -ne 0 -or -not $secret) { throw "cannot create the key" }
$env:BIRCHPAD_UPDATE_KEY = $secret
$public = (& $tool public-key | Out-String).Trim()
Remove-Item Env:BIRCHPAD_UPDATE_KEY
if ($LASTEXITCODE -ne 0 -or -not $public) { throw "cannot read the public key" }

Write-Host ""
Write-Host "The secret key (keep it in a password manager or offline; it is not saved anywhere):" -ForegroundColor Yellow
Write-Host ""
Write-Host "    $secret"
Write-Host ""
Read-Host "Press Enter once it is stored, to put it into the repository's settings and clear the screen"
Clear-Host

# The environment can then get protection rules (required reviewers, allowed branches) in the
# repository settings.
gh api --method PUT "repos/$Repo/environments/$Environment" --silent
if ($LASTEXITCODE -ne 0) { throw "cannot create the $Environment environment" }
$secret | gh secret set BIRCHPAD_UPDATE_KEY --repo $Repo --env $Environment
if ($LASTEXITCODE -ne 0) { throw "cannot set the secret" }
Remove-Variable secret
gh variable set BIRCHPAD_TRUSTED_KEYS --repo $Repo --body $public
if ($LASTEXITCODE -ne 0) { throw "cannot set the BIRCHPAD_TRUSTED_KEYS variable" }

Write-Host "BIRCHPAD_UPDATE_KEY is set in the $Environment environment of $Repo."
Write-Host "BIRCHPAD_TRUSTED_KEYS is $public; builds from now on trust it."
