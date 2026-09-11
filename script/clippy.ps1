#Requires -Version 7.4
$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

$clippyArgs = @($args)
$packageSpecified = $false
foreach ($arg in $clippyArgs) {
    if ($arg -ceq "-p" -or $arg -ceq "--package" -or $arg -clike "--package=*") {
        $packageSpecified = $true
        break
    }
}

if (-not $packageSpecified) {
    $clippyArgs += "--workspace"
}

cargo clippy @clippyArgs --release --all-targets --all-features -- --deny warnings
