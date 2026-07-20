#Requires -Version 7.4
#Requires -Modules @{ ModuleName = "PSScriptAnalyzer"; RequiredVersion = "1.25.0" }
param(
    [Alias("h")]
    [switch]$Help,
    [switch]$Verbose
)

if ($args.Length -gt 0) {
    Write-Error "Unexpected argument: $($args[0])"
    Write-Error "Usage: .\script\check.ps1 [-Verbose]"
    exit 1
}

if ($Help) {
    Write-Output "Usage: .\script\check.ps1 [-Verbose]"
    Write-Output "Check PowerShell scripts with PSScriptAnalyzer."
    exit 0
}

$ErrorActionPreference = "Stop"

$powerShellScripts = @(Get-ChildItem -Path $PSScriptRoot -Filter "*.ps1" -File | Sort-Object -Property Name)
if ($Verbose) {
    Write-Information "Checking $($powerShellScripts.Count) PowerShell scripts:" -InformationAction Continue
    foreach ($powerShellScript in $powerShellScripts) {
        Write-Information "  .\script\$($powerShellScript.Name)" -InformationAction Continue
    }
}
Invoke-ScriptAnalyzer -Path "$PSScriptRoot/*.ps1" -Severity Error, Warning -EnableExit
