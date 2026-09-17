# Fetch the Pdfium shared library the `pdf` input needs, from the
# open-source pdfium-binaries project's latest release, into
# .\data\pdfium\, and print the FLASH_PDFIUM_PATH line to set.
#
#   powershell -File tools\fetch-pdfium.ps1              detect the host
#   powershell -File tools\fetch-pdfium.ps1 win-arm64    one of: win-x64
#       win-arm64 linux-x64 linux-arm64 linux-musl-x64 linux-musl-arm64
#       mac-arm64 mac-x64
#
# Needs the tar that ships with Windows 10 and later. Re-run to update.
param(
    [string] $Platform = "",
    [string] $Dest = ".\data\pdfium"
)

$ErrorActionPreference = "Stop"

if ($Platform -eq "") {
    $arch = $env:PROCESSOR_ARCHITECTURE
    if ($env:PROCESSOR_ARCHITEW6432) { $arch = $env:PROCESSOR_ARCHITEW6432 }
    switch ($arch) {
        "AMD64" { $Platform = "win-x64" }
        "ARM64" { $Platform = "win-arm64" }
        default { throw "fetch-pdfium: no pdfium build for $arch; pass a platform name" }
    }
}

switch -Wildcard ($Platform) {
    "linux-*" { $member = "lib/libpdfium.so" }
    "mac-*" { $member = "lib/libpdfium.dylib" }
    "win-*" { $member = "bin/pdfium.dll" }
    default { throw "fetch-pdfium: unknown platform $Platform" }
}

if (-not (Get-Command tar -ErrorAction SilentlyContinue)) {
    throw "fetch-pdfium: needs tar (Windows 10 1803 or later ships it)"
}

$url = "https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-$Platform.tgz"
$tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("pdfium-" + [System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    Write-Host "fetch-pdfium: $url"
    $archive = Join-Path $tmp "pdfium.tgz"
    # Invoke-WebRequest's progress bar makes a large download crawl.
    $ProgressPreference = "SilentlyContinue"
    Invoke-WebRequest -Uri $url -OutFile $archive -UseBasicParsing

    & tar -xzf $archive -C $tmp $member VERSION
    if ($LASTEXITCODE -ne 0) { throw "fetch-pdfium: tar failed ($LASTEXITCODE)" }

    New-Item -ItemType Directory -Force -Path $Dest | Out-Null
    $name = Split-Path -Leaf $member
    Copy-Item (Join-Path $tmp $member) (Join-Path $Dest $name) -Force
    Copy-Item (Join-Path $tmp "VERSION") (Join-Path $Dest "VERSION") -Force

    $file = (Resolve-Path (Join-Path $Dest $name)).Path
    $build = (Get-Content (Join-Path $Dest "VERSION") | Where-Object { $_ -like "BUILD=*" }) -replace "^BUILD=", ""
    Write-Host "fetch-pdfium: chromium/$build -> $file"
    Write-Host ""
    Write-Host "FLASH_PDFIUM_PATH=$file"
}
finally {
    Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
