param(
    [string] $OutputDirectory = $PSScriptRoot
)

$ErrorActionPreference = 'Stop'

$archiveUrl = 'https://web.mit.edu/drela/Public/web/xfoil/xfoil6.99.tgz'
$archiveSha256 = '5C0250643F52CE0E75D7338AE2504CE7907F2D49A30F921826717B8AC12EBE40'
$mapSha256 = '2F6B3C63461D71DA9B6CB9CA1340D77CFF0CFBE767679D15B5B8556B45D948C4'
$licenseUrl = 'https://www.gnu.org/licenses/old-licenses/gpl-2.0.txt'
# SHA-256 of the licence text with LF line endings, as committed.
$licenseSha256 = 'EDAEF632CBB643E4E7A221717A6C441A4C1A7C918E6E4D56DEBC3D8739B233F6'

$output = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force -Path $output | Out-Null
$tempRoot = Join-Path ([IO.Path]::GetTempPath()) ('alas-xfoil-osmap-' + [guid]::NewGuid().ToString('N'))
$archive = Join-Path $tempRoot 'xfoil6.99.tgz'
$extract = Join-Path $tempRoot 'extract'
$licenseDownload = Join-Path $tempRoot 'COPYING-XFOIL.txt'

try {
    New-Item -ItemType Directory -Force -Path $extract | Out-Null
    Invoke-WebRequest -Uri $archiveUrl -OutFile $archive

    $actualArchiveSha256 = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash
    if ($actualArchiveSha256 -ne $archiveSha256) {
        throw "XFOIL archive SHA-256 mismatch: expected $archiveSha256, got $actualArchiveSha256"
    }

    tar -xzf $archive -C $extract
    if ($LASTEXITCODE -ne 0) {
        throw 'tar failed while extracting the official XFOIL archive'
    }

    $mapSource = Join-Path $extract 'Xfoil\orrs\osmapDP.dat'
    if (-not (Test-Path -LiteralPath $mapSource -PathType Leaf)) {
        throw "official archive did not contain $mapSource"
    }

    # Verify every file in the temporary folder; only then touch the output.
    $actualMapSha256 = (Get-FileHash -LiteralPath $mapSource -Algorithm SHA256).Hash
    if ($actualMapSha256 -ne $mapSha256) {
        throw "OSMAP SHA-256 mismatch: expected $mapSha256, got $actualMapSha256"
    }

    Invoke-WebRequest -Uri $licenseUrl -OutFile $licenseDownload
    $licenseBytes = [IO.File]::ReadAllBytes($licenseDownload)
    $licenseText = [Text.Encoding]::UTF8.GetString($licenseBytes).Replace("`r`n", "`n")
    $licenseNormalised = [Text.Encoding]::UTF8.GetBytes($licenseText)
    $actualLicenseSha256 = ([BitConverter]::ToString(
        [Security.Cryptography.SHA256]::Create().ComputeHash($licenseNormalised)) -replace '-', '')
    if ($actualLicenseSha256 -ne $licenseSha256) {
        throw "GPL-2.0 text SHA-256 mismatch: expected $licenseSha256, got $actualLicenseSha256"
    }

    Copy-Item -LiteralPath $archive -Destination (Join-Path $output 'xfoil6.99.tgz') -Force
    Copy-Item -LiteralPath $mapSource -Destination (Join-Path $output 'osmapDP.dat') -Force
    [IO.File]::WriteAllBytes((Join-Path $output 'COPYING-XFOIL.txt'), $licenseNormalised)

    Write-Output "Acquired official XFOIL 6.99 OSMAP: $actualMapSha256"
}
finally {
    $tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
    if ((Test-Path -LiteralPath $tempRoot) -and
        ([IO.Path]::GetFullPath($tempRoot)).StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase)) {
        Remove-Item -LiteralPath $tempRoot -Recurse -Force
    }
}
