#Requires -Version 7.4

if ($args.Length -ne 0) {
    Write-Error "Unexpected argument: $($args[0])"
    exit 1
}

$ErrorActionPreference = "Stop"

$cargo = if ($env:CARGO) { $env:CARGO } else { "cargo" }
$metadataJson = & $cargo metadata --no-deps --format-version=1
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

if ($version -match "^([0-9]+)\.([0-9]+)\.0(-.+)?$") {
    "$($Matches[1]).$($Matches[2])$($Matches[3])"
}
else {
    $version
}
