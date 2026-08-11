#Requires -Version 7.4
$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $true

$clippyArguments = @($args)
$packageSpecified = $false
foreach ($argument in $clippyArguments) {
    if ($argument -ceq "-p" -or $argument -ceq "--package" -or $argument -clike "--package=*") {
        $packageSpecified = $true
        break
    }
}

if (-not $packageSpecified) {
    $clippyArguments += "--workspace"
}

cargo clippy @clippyArguments --release --all-targets --all-features -- --deny warnings
