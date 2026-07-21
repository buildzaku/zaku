#Requires -Version 7.4
#Requires -Modules @{ ModuleName = "PSScriptAnalyzer"; RequiredVersion = "1.25.0" }
param(
    [Alias("h")]
    [switch]$Help,
    [switch]$Verbose
)

$workspaceDirectory = Split-Path -Parent $PSScriptRoot
$scriptPath = Resolve-Path -LiteralPath $PSCommandPath -RelativeBasePath $workspaceDirectory -Relative

if ($args.Length -gt 0) {
    Write-Error "Unexpected argument: $($args[0])"
    Write-Error "Run pwsh -File $scriptPath -Help"
    exit 1
}

if ($Help) {
    Write-Output "Usage: pwsh -File $scriptPath [OPTIONS]"
    Write-Output "Check PowerShell scripts with PSScriptAnalyzer."
    Write-Output "Options:"
    Write-Output "  -Verbose   List scripts being checked."
    Write-Output "  -h, -Help  Show help."
    exit 0
}

$ErrorActionPreference = "Stop"

$powerShellScripts = @(Get-ChildItem -Path $PSScriptRoot -Filter "*.ps1" -File | Sort-Object -Property Name)
if ($Verbose) {
    Write-Information "Checking $($powerShellScripts.Count) PowerShell scripts:" -InformationAction Continue
    foreach ($powerShellScript in $powerShellScripts) {
        Write-Information "  $(Resolve-Path -LiteralPath $powerShellScript.FullName -RelativeBasePath $workspaceDirectory -Relative)" -InformationAction Continue
    }
}
Invoke-ScriptAnalyzer -Path "$PSScriptRoot/*.ps1" -Severity Error, Warning -EnableExit
