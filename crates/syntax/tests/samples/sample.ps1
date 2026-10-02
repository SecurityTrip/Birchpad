# Lists files.
param([string]$Path = ".")
Get-ChildItem -Path $Path | Where-Object { $_.Length -gt 100 }
Write-Host "Done"
