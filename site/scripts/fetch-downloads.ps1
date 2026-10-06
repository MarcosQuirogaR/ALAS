<#
.SYNOPSIS
  Download the release archives (and .sha256 files) of one tag into a folder,
  normally <site>/dist/downloads, so they are deployed next to the site.

.EXAMPLE
  ./scripts/fetch-downloads.ps1 -OutDir dist/downloads
  ./scripts/fetch-downloads.ps1 -Tag v1.3.1 -OutDir dist/downloads

  Requires the GitHub CLI (gh), authenticated if the repository needs it.
  Keep -Tag in step with CURRENT in src/lib/releases.ts (the newest entry with
  a non-null `published`); the default reads it from that file.
#>
param(
  [string]$Tag = "",
  [Parameter(Mandatory = $true)][string]$OutDir,
  [string]$Repo = "MarcosQuirogaR/ALAS"
)
$ErrorActionPreference = "Stop"

if (-not $Tag) {
  # First entry with a date: find the first tag whose block has published: '...'.
  $src = Get-Content -Raw (Join-Path $PSScriptRoot "../src/lib/releases.ts")
  $m = [regex]::Match($src, "tag:\s*'(v[0-9.]+)',\s*(?://[^\r\n]*\s*)*published:\s*'")
  if (-not $m.Success) { throw "Could not determine the current tag from releases.ts; pass -Tag." }
  $Tag = $m.Groups[1].Value
}

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
Write-Host "Fetching $Tag assets from $Repo into $OutDir"
gh release download $Tag -R $Repo -D $OutDir --clobber `
  -p "alas-*-windows-x86_64.zip*" -p "alas-*-linux-x86_64.tar.gz*"
if ($LASTEXITCODE -ne 0) { throw "gh release download failed" }

# Verify each archive against its sibling .sha256.
foreach ($sum in Get-ChildItem $OutDir -Filter "*.sha256") {
  $expected = ((Get-Content $sum.FullName -TotalCount 1) -split "\s+")[0].ToLower()
  $archive = Join-Path $OutDir ($sum.Name -replace "\.sha256$", "")
  $actual = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLower()
  if ($expected -ne $actual) { throw "SHA-256 mismatch for $archive" }
  Write-Host "OK  $($sum.Name -replace '\.sha256$', '')  $actual"
}
