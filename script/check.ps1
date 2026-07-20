#Requires -Version 7.4
#Requires -Modules @{ ModuleName = "PSScriptAnalyzer"; RequiredVersion = "1.25.0" }
$ErrorActionPreference = "Stop"

if ($args.Length -gt 0) {
    Write-Error "Unexpected argument: $($args[0])"
    exit 1
}

Invoke-ScriptAnalyzer -Path $PSScriptRoot -Severity Error, Warning -EnableExit
