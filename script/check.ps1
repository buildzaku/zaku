#Requires -Version 7.4
#Requires -Modules @{ ModuleName = "PSScriptAnalyzer"; RequiredVersion = "1.25.0" }
$ErrorActionPreference = "Stop"

if ($args.Length -gt 0) {
    Write-Error "Unexpected argument: $($args[0])"
    exit 1
}

$powerShellScripts = @(Get-ChildItem -Path $PSScriptRoot -Filter "*.ps1" -File)
Write-Information "Checking $($powerShellScripts.Count) PowerShell scripts" -InformationAction Continue
Invoke-ScriptAnalyzer -Path "$PSScriptRoot/*.ps1" -Severity Error, Warning -EnableExit
