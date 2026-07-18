if ($args.Length -ne 0) {
    Write-Error "Usage: $($MyInvocation.MyCommand.Name)"
    exit 1
}

$cargo = if ($env:CARGO) { $env:CARGO } else { "cargo" }
$metadataJson = & $cargo metadata --no-deps --format-version=1
if ($LASTEXITCODE -ne 0) {
    throw "Could not read the Cargo workspace metadata"
}
$metadata = $metadataJson | ConvertFrom-Json
$package = $metadata.packages | Where-Object { $_.name -eq "zaku" }
if (-not $package) {
    throw "Could not find the Zaku package"
}

$version = $package.version
if ($version -match "^([0-9]+)\.([0-9]+)\.0$") {
    "$($Matches[1]).$($Matches[2])"
}
else {
    $version
}
