#Requires -Version 7.4
$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

$cargo = if ($env:CARGO) {
    $env:CARGO
}
else {
    "cargo"
}

$needAddWorkspace = $false
if ($args -cnotcontains "-p" -and $args -cnotcontains "--package") {
    $needAddWorkspace = $true
}

if ($needAddWorkspace) {
    & $cargo clippy @args --workspace --release --all-targets --all-features -- --deny warnings
}
else {
    & $cargo clippy @args --release --all-targets --all-features -- --deny warnings
}

if (Get-Command typos -ErrorAction Ignore) {
    & typos
}
