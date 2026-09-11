#Requires -Version 7.4
#Requires -Modules @{ ModuleName = "PSScriptAnalyzer"; RequiredVersion = "1.25.0" }
param(
    [Alias("h")]
    [switch]$Help,
    [switch]$Check
)

if ($Help) {
    Write-Output "Fix or check PowerShell script formatting."
    Write-Output ""
    Write-Output "Usage: ./script/fmt-pwsh.ps1 [OPTIONS]"
    Write-Output ""
    Write-Output "Options:"
    Write-Output "  -Check     Check formatting without writing changes."
    Write-Output "  -h, -Help  Show help."
    exit 0
}

if ($args.Length -gt 0) {
    Write-Error "Unexpected argument: $($args[0])"
    Write-Error "Run ./script/fmt-pwsh.ps1 -Help"
    exit 1
}

$ErrorActionPreference = "Stop"

$powerShellScripts = @(Get-ChildItem -Path "script" -Filter "*.ps1" -File | Sort-Object -Property Name)
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
        $relativePath = "./script/$($powerShellScript.Name)"
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
    Write-Output "Run ./script/fmt-pwsh.ps1"
    exit 1
}
