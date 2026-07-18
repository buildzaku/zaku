#Requires -Version 7.4
[CmdletBinding()]
param(
    [Parameter()]
    [ValidateSet("x86_64", "aarch64")]
    [string]$Architecture
)

$ErrorActionPreference = "Stop"

if (-not [System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
        [System.Runtime.InteropServices.OSPlatform]::Windows
    )) {
    throw "Windows application bundles must be built on Windows"
}

$hostArchitecture = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture) {
    "X64" { "x86_64" }
    "Arm64" { "aarch64" }
    default { throw "Unsupported Windows architecture" }
}
if (-not $Architecture) {
    $Architecture = $hostArchitecture
}

$target = "$Architecture-pc-windows-msvc"
$workspaceDirectory = Split-Path -Parent $PSScriptRoot
$targetDirectory = if ($env:CARGO_TARGET_DIR) {
    $env:CARGO_TARGET_DIR
}
else {
    Join-Path $workspaceDirectory "target"
}
$releaseDirectory = Join-Path $targetDirectory "$target/release"
$bundleDirectory = Join-Path $releaseDirectory "bundle/windows"
$sourceDirectory = Join-Path $bundleDirectory "source"
$outputDirectory = Join-Path $bundleDirectory "output"
$cargo = if ($env:CARGO) { $env:CARGO } else { "cargo" }

$iscc = if ($env:ISCC_PATH) {
    $env:ISCC_PATH
}
else {
    $compiler = @(
        (Join-Path $env:ProgramFiles "Inno Setup 7/ISCC.exe")
        (Join-Path $env:LOCALAPPDATA "Programs/Inno Setup 7/ISCC.exe")
    ) | Where-Object { Test-Path $_ -PathType Leaf } | Select-Object -First 1
    if ($compiler) {
        $compiler
    }
    else {
        (Get-Command "ISCC.exe" -ErrorAction SilentlyContinue).Source
    }
}
if (-not $iscc -or -not (Test-Path $iscc -PathType Leaf)) {
    throw "Inno Setup 7 compiler was not found. Install Inno Setup 7 or set ISCC_PATH to ISCC.exe"
}

$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio/Installer/vswhere.exe"
if (-not (Test-Path $vswhere -PathType Leaf)) {
    throw "Visual Studio Installer could not be found"
}
$visualStudioComponent = if ($Architecture -eq "x86_64") {
    "Microsoft.VisualStudio.Component.VC.Tools.x86.x64"
}
else {
    "Microsoft.VisualStudio.Component.VC.Tools.ARM64"
}
$visualStudioDirectory = & $vswhere -latest -products * -requires $visualStudioComponent -property installationPath
if ($LASTEXITCODE -ne 0) {
    throw "Visual Studio discovery failed"
}
if (-not $visualStudioDirectory) {
    throw "Visual Studio with the MSVC build tools could not be found"
}
$developerShell = Join-Path $visualStudioDirectory "Common7/Tools/Launch-VsDevShell.ps1"
$visualStudioArchitecture = if ($Architecture -eq "x86_64") { "amd64" } else { "arm64" }
$visualStudioHostArchitecture = if ($hostArchitecture -eq "x86_64") { "amd64" } else { "arm64" }
& $developerShell -Arch $visualStudioArchitecture -HostArch $visualStudioHostArchitecture -SkipAutomaticLocation

Push-Location $workspaceDirectory
try {
    $version = & "$PSScriptRoot/get-version.ps1"
    if (-not $version) {
        throw "Could not read the Zaku package version"
    }
    $versionCore = ($version -split "-", 2)[0]
    $versionInfoVersion = switch ($versionCore.Split(".").Length) {
        2 { "$versionCore.0.0" }
        3 { "$versionCore.0" }
        default { throw "Invalid Zaku version: $version" }
    }

    Write-Output "Compiling Zaku"
    rustup target add $target
    if ($LASTEXITCODE -ne 0) {
        throw "Could not install the Rust target $target"
    }
    & $cargo build --release --package zaku --package updater_windows --target $target
    if ($LASTEXITCODE -ne 0) {
        throw "Could not compile Zaku for $target"
    }

    if (Test-Path $bundleDirectory) {
        Remove-Item $bundleDirectory -Recurse -Force
    }
    New-Item (Join-Path $sourceDirectory "tools") -ItemType Directory -Force | Out-Null
    New-Item $outputDirectory -ItemType Directory -Force | Out-Null

    Copy-Item (Join-Path $releaseDirectory "zaku.exe") (Join-Path $sourceDirectory "Zaku.exe")
    Copy-Item (Join-Path $releaseDirectory "updater_windows.exe") (Join-Path $sourceDirectory "tools/updater_windows.exe")
    Copy-Item "crates/zaku/resources/windows/app-icon.ico" (Join-Path $sourceDirectory "app-icon.ico")

    & $iscc "/DArchitecture=$Architecture" "/DVersion=$version" "/DVersionInfoVersion=$versionInfoVersion" "/DSourceDir=$sourceDirectory" "/DOutputDir=$outputDirectory" "crates/zaku/resources/windows/zaku.iss"
    if ($LASTEXITCODE -ne 0) {
        throw "Could not create the Zaku installer"
    }

    $installer = Join-Path $outputDirectory "Zaku-$version-$Architecture.exe"
    if (-not (Test-Path $installer -PathType Leaf)) {
        throw "Zaku installer was not created at $installer"
    }
    $artifact = Join-Path $releaseDirectory "Zaku-$version-$Architecture.exe"
    Move-Item $installer $artifact -Force
    Write-Output "Created Windows installer: $artifact"
}
finally {
    Pop-Location
}
