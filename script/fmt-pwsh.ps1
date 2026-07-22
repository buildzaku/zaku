#Requires -Version 7.4
#Requires -Modules @{ ModuleName = "PSScriptAnalyzer"; RequiredVersion = "1.25.0" }
param(
    [Alias("h")]
    [switch]$Help,
    [switch]$Check
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
    Write-Output "Fix or check PowerShell script formatting."
    Write-Output "Options:"
    Write-Output "  -Check     Check formatting without writing changes."
    Write-Output "  -h, -Help  Show help."
    exit 0
}

$ErrorActionPreference = "Stop"

$powerShellScripts = @(Get-ChildItem -Path $PSScriptRoot -Filter "*.ps1" -File | Sort-Object -Property Name)
$unformattedPowerShellScripts = @()
$utf8Encoding = [System.Text.UTF8Encoding]::new($false)
$additionalFormattingSettings = @{
    IncludeRules = @(
        "PSAvoidSemicolonsAsLineTerminators"
        "PSAvoidExclaimOperator"
        "PSAvoidTrailingWhitespace"
    )
    Rules        = @{
        PSAvoidSemicolonsAsLineTerminators = @{
            Enable = $true
        }
        PSAvoidExclaimOperator             = @{
            Enable = $true
        }
        PSAvoidTrailingWhitespace          = @{}
    }
}

foreach ($powerShellScript in $powerShellScripts) {
    $source = [System.IO.File]::ReadAllText($powerShellScript.FullName)
    $formattedSource = Invoke-Formatter -ScriptDefinition $source -Settings "CodeFormatting"
    $formattedSource = Invoke-Formatter -ScriptDefinition $formattedSource -Settings $additionalFormattingSettings
    $formattedSource = $formattedSource.Replace("`r`n", "`n").Replace("`r", "`n")

    if ($source -cne $formattedSource) {
        $relativePath = Resolve-Path -LiteralPath $powerShellScript.FullName -RelativeBasePath $workspaceDirectory -Relative
        if ($Check) {
            $unformattedPowerShellScripts += $relativePath
        }
        else {
            [System.IO.File]::WriteAllText($powerShellScript.FullName, $formattedSource, $utf8Encoding)
            Write-Output "Formatted $relativePath"
        }
    }
}

if ($unformattedPowerShellScripts.Length -gt 0) {
    Write-Output "PowerShell scripts need formatting:"
    foreach ($unformattedPowerShellScript in $unformattedPowerShellScripts) {
        Write-Output "  $unformattedPowerShellScript"
    }
    Write-Output "Run ./script/fmt"
    exit 1
}
