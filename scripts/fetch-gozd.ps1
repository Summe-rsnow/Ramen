<#
.SYNOPSIS
    Downloads and verifies the pinned gozd.exe daemon binary for Ramen.

.DESCRIPTION
    Fetches goz v0.7.1 Windows x86_64 archive from GitHub Releases, verifies its
    SHA-256 digest against the pinned checksum, and extracts gozd.exe atomically
    into the target output directory.

.EXAMPLE
    .\scripts\fetch-gozd.ps1
    .\scripts\fetch-gozd.ps1 -OutDir .nsh/bundle -Force
#>
[CmdletBinding()]
param(
    [string]$OutDir = (Join-Path (Split-Path -Parent $PSScriptRoot) "target\release"),
    [switch]$Force
)

$ErrorActionPreference = "Stop"

$PinnedTag = "v0.7.1"
$PinnedUrl = "https://github.com/mustafaahci/goz/releases/download/$PinnedTag/goz-$PinnedTag-windows-x86_64.zip"
$PinnedSha256 = "7044c8d2938b7b684818140c86c61042969548b0c62d46faf68deed216460220"

New-Item -ItemType Directory -Force -Path $OutDir | Out-Null
$TargetExe = Join-Path $OutDir "gozd.exe"

if ((Test-Path $TargetExe) -and -not $Force) {
    Write-Host "gozd.exe already present at $TargetExe (pass -Force to re-download)." -ForegroundColor Cyan
    exit 0
}

$TempZip = Join-Path ([System.IO.Path]::GetTempPath()) ("goz-$PinnedTag-" + [System.Guid]::NewGuid().ToString("N") + ".zip")
$TempExe = "$TargetExe.tmp"

try {
    Write-Host "Downloading $PinnedUrl ..." -ForegroundColor Cyan
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri $PinnedUrl -OutFile $TempZip -UseBasicParsing

    $ActualHash = (Get-FileHash -Path $TempZip -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($ActualHash -ne $PinnedSha256) {
        throw "SHA-256 mismatch for $PinnedUrl! Expected $PinnedSha256, got $ActualHash"
    }
    Write-Host "Verified SHA-256: $ActualHash" -ForegroundColor Green

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $Archive = [System.IO.Compression.ZipFile]::OpenRead($TempZip)
    try {
        $Entry = $Archive.Entries | Where-Object {
            $_.Name -ieq "gozd.exe"
        } | Select-Object -First 1

        if (-not $Entry) {
            throw "gozd.exe not found inside $PinnedUrl"
        }

        $InStream = $Entry.Open()
        try {
            $OutStream = [System.IO.File]::Create($TempExe)
            try {
                $InStream.CopyTo($OutStream)
            } finally {
                $OutStream.Dispose()
            }
        } finally {
            $InStream.Dispose()
        }
    } finally {
        $Archive.Dispose()
    }

    Move-Item -Path $TempExe -Destination $TargetExe -Force
    Write-Host "Extracted gozd.exe ($PinnedTag) -> $TargetExe" -ForegroundColor Green
} finally {
    if (Test-Path $TempZip) {
        Remove-Item -Path $TempZip -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path $TempExe) {
        Remove-Item -Path $TempExe -Force -ErrorAction SilentlyContinue
    }
}
