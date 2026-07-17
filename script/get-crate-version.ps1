if ($args.Length -ne 1) {
    Write-Error "Usage: $($MyInvocation.MyCommand.Name) <crate_name>"
    exit 1
}

$crateName = $args[0]
$cargo = if ($env:CARGO) { $env:CARGO } else { "cargo" }
$metadataJson = & $cargo metadata --no-deps --format-version=1
if ($LASTEXITCODE -ne 0) {
    throw "Could not read the Cargo workspace metadata"
}
$metadata = $metadataJson | ConvertFrom-Json

$package = $metadata.packages | Where-Object { $_.name -eq $crateName }
if ($package) {
    $package.version
}
else {
    Write-Error "Crate '$crateName' not found."
}
