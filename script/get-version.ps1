#Requires -Version 7.4
[CmdletBinding()]
param(
    [Parameter()]
    [Alias("h")]
    [switch]$Help,
    [Parameter()]
    [switch]$Display
)

$workspaceDirectory = Split-Path -Parent $PSScriptRoot
$scriptPath = Resolve-Path -LiteralPath $PSCommandPath -RelativeBasePath $workspaceDirectory -Relative

if ($args.Length -gt 0) {
    Write-Error "Unexpected argument: $($args[0])"
    Write-Error "Run pwsh -File $scriptPath -Help"
    exit 1
}

if ($Help) {
    Write-Output "Usage: pwsh -File $scriptPath [OPTIONS]"
    Write-Output "Print Zaku version."
    Write-Output "Options:"
    Write-Output "  -Display   Print Zaku display version."
    Write-Output "  -h, -Help  Show help."
    exit 0
}

$ErrorActionPreference = "Stop"

$metadataJson = cargo metadata --no-deps --format-version=1
if ($LASTEXITCODE -ne 0) {
    throw "Could not read the Cargo workspace metadata"
}
$metadata = $metadataJson | ConvertFrom-Json
$package = $metadata.packages | Where-Object { $_.name -ceq "zaku" }
if (-not $package) {
    throw "Could not find the Zaku package"
}

$version = $package.version
if ($version.Contains("+")) {
    throw "Version cannot contain build metadata"
}

if ($Display -and $version -match "^([0-9]+)\.([0-9]+)\.0(-.+)?$") {
    "$($Matches[1]).$($Matches[2])$($Matches[3])"
}
else {
    $version
}
