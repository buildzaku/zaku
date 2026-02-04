$ErrorActionPreference = 'Stop'

$Cargo = if ($env:CARGO) {
    $env:CARGO
}
elseif ($cmd = Get-Command cargo -ErrorAction Ignore) {
    $cmd.Source
}
else {
    throw 'Could not find cargo in path.'
}

$needAddWorkspace = $false
if ($args -notcontains "-p" -and $args -notcontains "--package") {
    $needAddWorkspace = $true
}

if ($needAddWorkspace) {
    & $Cargo clippy --workspace --release --all-targets --all-features -- --deny warnings
}
else {
    & $Cargo clippy --release --all-targets --all-features -- --deny warnings
}

if (Get-Command typos -ErrorAction Ignore) {
    & typos
}
