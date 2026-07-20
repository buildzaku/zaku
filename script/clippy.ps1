#Requires -Version 7.4
$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

$Cargo = if ($env:CARGO) {
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
    & $Cargo clippy @args --workspace --release --all-targets --all-features -- --deny warnings
}
else {
    & $Cargo clippy @args --release --all-targets --all-features -- --deny warnings
}

if (Get-Command typos -ErrorAction Ignore) {
    & typos
}
