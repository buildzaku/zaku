#Requires -Version 7.4
#Requires -Modules @{ ModuleName = "PSScriptAnalyzer"; RequiredVersion = "1.25.0" }
param(
    [Alias("h")]
    [switch]$Help,
    [switch]$Verbose
)

if ($Help) {
    Write-Output "Check PowerShell scripts with PSScriptAnalyzer."
    Write-Output ""
    Write-Output "Usage: ./script/check-pwsh.ps1 [OPTIONS]"
    Write-Output ""
    Write-Output "Options:"
    Write-Output "  -Verbose   List scripts being checked."
    Write-Output "  -h, -Help  Show help."
    exit 0
}

if ($args.Length -gt 0) {
    Write-Error "Unexpected argument: $($args[0])"
    Write-Error "Run ./script/check-pwsh.ps1 -Help"
    exit 1
}

$ErrorActionPreference = "Stop"

$powerShellScripts = @(Get-ChildItem -Path "script" -Filter "*.ps1" -File | Sort-Object -Property Name)
if ($Verbose) {
    Write-Information "Checking $($powerShellScripts.Count) PowerShell scripts:" -InformationAction Continue
    foreach ($powerShellScript in $powerShellScripts) {
        Write-Information "  ./script/$($powerShellScript.Name)" -InformationAction Continue
    }
}
Invoke-ScriptAnalyzer -Path "script/*.ps1" -Severity Error, Warning -EnableExit
