#Requires -Version 7.4
#Requires -Modules @{ ModuleName = "PSScriptAnalyzer"; RequiredVersion = "1.25.0" }
$ErrorActionPreference = "Stop"

if ($args.Length -gt 0) {
    Write-Error "Unexpected argument: $($args[0])"
    exit 1
}

$powerShellScripts = @(Get-ChildItem -Path $PSScriptRoot -Filter "*.ps1" -File | Sort-Object -Property Name)
Write-Information "Checking PowerShell scripts:" -InformationAction Continue
foreach ($powerShellScript in $powerShellScripts) {
    Write-Information "  .\script\$($powerShellScript.Name)" -InformationAction Continue
}
Invoke-ScriptAnalyzer -Path "$PSScriptRoot/*.ps1" -Severity Error, Warning -EnableExit
