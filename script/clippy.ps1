#Requires -Version 7.4
$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

$shouldAddWorkspace = $false
if ($args -cnotcontains "-p" -and $args -cnotcontains "--package") {
    $shouldAddWorkspace = $true
}

if ($shouldAddWorkspace) {
    cargo clippy @args --workspace --release --all-targets --all-features -- --deny warnings
}
else {
    cargo clippy @args --release --all-targets --all-features -- --deny warnings
}

if (Get-Command typos -ErrorAction Ignore) {
    & typos
}
