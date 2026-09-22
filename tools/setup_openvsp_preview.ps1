# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez
<#
.SYNOPSIS
Provision the optional, app-local Windows x64 OpenVSP screenshot runtime.
.DESCRIPTION
Downloads hash-pinned archives; never modifies system Python or PATH. Existing
destinations are refused unless -Force is set, in which case they are backed up.
An existing cache may contain python.zip, openvsp-python.zip and numpy.zip.
#>
[CmdletBinding()]
param(
    [string]$Destination = (Join-Path $PSScriptRoot '../external tools/OpenVSP-3.51.2-win64/preview-runtime'),
    [string]$CacheDirectory = (Join-Path $PSScriptRoot '../out/openvsp-runtime'),
    [switch]$Force
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($env:OS -ne 'Windows_NT' -or -not [Environment]::Is64BitOperatingSystem) {
    throw 'This runtime requires 64-bit Windows.'
}
Add-Type -AssemblyName System.IO.Compression.FileSystem

$destinationPath = [IO.Path]::GetFullPath($Destination).TrimEnd('\', '/')
$cachePath = [IO.Path]::GetFullPath($CacheDirectory)
$parentPath = [IO.Path]::GetDirectoryName($destinationPath)
if ([string]::IsNullOrWhiteSpace($parentPath) -or
    $destinationPath -eq [IO.Path]::GetPathRoot($destinationPath).TrimEnd('\', '/')) {
    throw 'Destination must be a dedicated runtime subdirectory, not a drive root.'
}
if (Test-Path -LiteralPath $destinationPath) {
    $existing = Get-Item -LiteralPath $destinationPath
    if (-not $existing.PSIsContainer -or
        ($existing.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw 'An existing destination must be an ordinary directory.'
    }
    if (-not $Force) { throw "Destination already exists: $destinationPath. Use -Force to back it up and replace it." }
}

$packages = @(
    @{
        Name = 'python.zip'
        Url = 'https://www.python.org/ftp/python/3.13.7/python-3.13.7-embed-amd64.zip'
        Sha256 = 'F6CCA216A359BE84797CABB54149CE5E062AFB16CC7567EB7FC51CACB2D86B65'
    },
    @{
        Name = 'openvsp-python.zip'
        Url = 'https://openvsp.org/zips/old/windows/OpenVSP-3.51.2-win64-Python3.13.zip'
        Sha256 = '4D08134F5A7FF5B244FE47F557AD5B40787DF631234017EA5B0B831117954252'
    },
    @{
        Name = 'numpy.zip'
        Url = 'https://files.pythonhosted.org/packages/1b/b5/263ebbbbcede85028f30047eab3d58028d7ebe389d6493fc95ae66c636ab/numpy-2.3.3-cp313-cp313-win_amd64.whl'
        Sha256 = 'F0DADEB302887F07431910F67A14D57209ED91130BE0ADEA2F9793F1A4F817CF'
    }
)

function Get-VerifiedArchive($Package) {
    $archivePath = Join-Path $cachePath $Package.Name
    if (-not (Test-Path -LiteralPath $archivePath -PathType Leaf)) {
        Write-Host "Downloading $($Package.Name)..."
        Invoke-WebRequest -Uri $Package.Url -OutFile $archivePath -UseBasicParsing
    }
    $actual = (Get-FileHash -LiteralPath $archivePath -Algorithm SHA256).Hash
    if ($actual -ne $Package.Sha256) {
        throw "SHA-256 mismatch for $archivePath. Expected $($Package.Sha256), got $actual. Remove the invalid cache file before retrying."
    }
    return $archivePath
}

function Expand-SafeArchive([string]$ArchivePath, [string]$OutputPath, [scriptblock]$MapName) {
    $root = [IO.Path]::GetFullPath($OutputPath).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $archive = [IO.Compression.ZipFile]::OpenRead($ArchivePath)
    try {
        foreach ($entry in $archive.Entries) {
            $name = $entry.FullName.Replace('\', '/')
            # Validate source names even for entries excluded by the mapper.
            if ($name.StartsWith('/') -or $name.Contains(':') -or
                ($name.Split('/') -contains '..')) {
                throw "Unsafe archive entry: $name"
            }
            if ($name.EndsWith('/')) { continue }
            $mapped = & $MapName $name
            if ([string]::IsNullOrEmpty($mapped)) { continue }
            $target = [IO.Path]::GetFullPath((Join-Path $root $mapped))
            if (-not $target.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) {
                throw "Archive entry escapes runtime directory: $name"
            }
            [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($target)) | Out-Null
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $target, $true)
        }
    }
    finally { $archive.Dispose() }
}

[IO.Directory]::CreateDirectory($cachePath) | Out-Null
$archives = @($packages | ForEach-Object { Get-VerifiedArchive $_ })
[IO.Directory]::CreateDirectory($parentPath) | Out-Null
$stagingPath = Join-Path $parentPath ('.preview-runtime-stage-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($stagingPath) | Out-Null

try {
    Expand-SafeArchive $archives[0] $stagingPath { param($name) return $name }
    Expand-SafeArchive $archives[1] $stagingPath {
        param($name)
        $prefix = 'OpenVSP-3.51.2-win64/'
        if (-not $name.StartsWith($prefix, [StringComparison]::Ordinal)) {
            throw "Unexpected OpenVSP archive layout: $name"
        }
        $relative = $name.Substring($prefix.Length)
        if ($relative -eq 'LICENSE') { return 'LICENSE.OpenVSP.txt' }
        if ($relative -eq 'python/LICENSE') { return 'python/LICENSE' }
        if ($relative.StartsWith('python/openvsp/') -or $relative.StartsWith('python/openvsp_config/') -or
            ($relative -notmatch '/' -and $relative.EndsWith('.dll', [StringComparison]::OrdinalIgnoreCase))) {
            return $relative
        }
        return $null
    }
    Expand-SafeArchive $archives[2] $stagingPath { param($name) return $name }
    [IO.File]::WriteAllLines((Join-Path $stagingPath 'python313._pth'), @(
        'python313.zip', '.', 'python/openvsp', 'python/openvsp_config'
    ), [Text.Encoding]::ASCII)
    foreach ($required in @('python.exe', 'python313.dll', 'python/openvsp/openvsp/vsp.py',
            'numpy/__init__.py', 'LICENSE.txt', 'LICENSE.OpenVSP.txt')) {
        if (-not (Test-Path -LiteralPath (Join-Path $stagingPath $required) -PathType Leaf)) {
            throw "Runtime assembly is missing $required"
        }
    }
    $manifest = @{ Python = '3.13.7'; OpenVSP = '3.51.2'; NumPy = '2.3.3'; Archives = $packages }
    [IO.File]::WriteAllText((Join-Path $stagingPath 'runtime-manifest.json'),
        ($manifest | ConvertTo-Json -Depth 5), [Text.Encoding]::UTF8)

    if (Test-Path -LiteralPath $destinationPath) {
        # Both final absolute paths are checked before moving the explicitly
        # selected destination. No recursive deletion is used.
        $backupPath = $destinationPath + '.backup-' + [Guid]::NewGuid().ToString('N')
        if ([IO.Path]::GetDirectoryName($backupPath) -ne $parentPath) {
            throw 'Backup directory is outside the destination parent.'
        }
        Move-Item -LiteralPath $destinationPath -Destination $backupPath
        Write-Host "Previous runtime preserved at $backupPath"
    }
    if ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($stagingPath)) -ne $parentPath) {
        throw 'Staging directory is outside the destination parent.'
    }
    Move-Item -LiteralPath $stagingPath -Destination $destinationPath
    Write-Host "OpenVSP preview runtime installed at $destinationPath"
}
catch {
    Write-Warning "Setup failed. Any staged files remain at $stagingPath for inspection."
    throw
}
