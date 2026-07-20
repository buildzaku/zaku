#Requires -Version 7.4
$ErrorActionPreference = "Stop"

$SCCACHE_VERSION = "v0.16.0"
$SCCACHE_DIR = "./target/sccache"

function Install-Sccache {
    New-Item -ItemType Directory -Path $SCCACHE_DIR -Force | Out-Null

    $sccachePath = Join-Path $SCCACHE_DIR "sccache.exe"

    if (Test-Path $sccachePath) {
        Write-Information "sccache already cached: $(& $sccachePath --version)" -InformationAction Continue
    }
    else {
        Write-Information "Installing sccache ${SCCACHE_VERSION} from GitHub releases..." -InformationAction Continue

        $architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
        $arch = switch ($architecture) {
            "Arm64" { "aarch64" }
            "X64" { "x86_64" }
            default {
                throw "Unsupported Windows architecture: $architecture"
            }
        }
        $archive = "sccache-${SCCACHE_VERSION}-${arch}-pc-windows-msvc.zip"
        $basename = "sccache-${SCCACHE_VERSION}-${arch}-pc-windows-msvc"
        $url = "https://github.com/mozilla/sccache/releases/download/${SCCACHE_VERSION}/${archive}"

        $tempDir = Join-Path $env:TEMP "sccache-install"
        New-Item -ItemType Directory -Path $tempDir -Force | Out-Null

        try {
            $archivePath = Join-Path $tempDir $archive
            Invoke-WebRequest -Uri $url -OutFile $archivePath
            Expand-Archive -Path $archivePath -DestinationPath $tempDir

            $extractedPath = Join-Path $tempDir $basename "sccache.exe"
            Move-Item -Path $extractedPath -Destination $sccachePath -Force

            Write-Information "Installed sccache: $(& $sccachePath --version)" -InformationAction Continue
        }
        finally {
            Remove-Item -Recurse -Force $tempDir -ErrorAction SilentlyContinue
        }
    }

    $absolutePath = (Resolve-Path $SCCACHE_DIR).Path
    if ($env:GITHUB_PATH) {
        $absolutePath | Out-File -FilePath $env:GITHUB_PATH -Append -Encoding utf8
    }
    $env:PATH = "$absolutePath;$env:PATH"

    $sccacheCommand = Get-Command sccache -ErrorAction SilentlyContinue
    if (-not $sccacheCommand) {
        throw "Could not find sccache in PATH after installing it at $absolutePath"
    }
}

function Initialize-SccacheEnvironment {
    $missing = @()

    foreach ($name in @("R2_ACCOUNT_ID", "R2_ACCESS_KEY_ID", "R2_SECRET_ACCESS_KEY", "R2_SCCACHE_BUCKET")) {
        if (-not [Environment]::GetEnvironmentVariable($name)) {
            $missing += $name
        }
    }

    if ($missing.Length -gt 0) {
        Write-Information "Missing $($missing -join ', '), skipping sccache configuration" -InformationAction Continue
        return
    }

    $sccacheCommand = Get-Command sccache -ErrorAction SilentlyContinue
    if (-not $sccacheCommand) {
        throw "Could not find sccache in PATH while configuring RUSTC_WRAPPER"
    }

    Write-Information "Configuring sccache with Cloudflare R2..." -InformationAction Continue

    $baseDir = if ($env:GITHUB_WORKSPACE) { $env:GITHUB_WORKSPACE } else { (Get-Location).Path }
    $sccacheBin = $sccacheCommand.Source

    $env:SCCACHE_ENDPOINT = "https://$($env:R2_ACCOUNT_ID).r2.cloudflarestorage.com"
    $env:SCCACHE_BUCKET = $env:R2_SCCACHE_BUCKET
    $env:SCCACHE_REGION = "auto"
    $env:SCCACHE_BASEDIRS = $baseDir
    $env:AWS_ACCESS_KEY_ID = $env:R2_ACCESS_KEY_ID
    $env:AWS_SECRET_ACCESS_KEY = $env:R2_SECRET_ACCESS_KEY
    $env:RUSTC_WRAPPER = $sccacheBin

    if ($env:GITHUB_ENV) {
        @(
            "SCCACHE_ENDPOINT=$($env:SCCACHE_ENDPOINT)"
            "SCCACHE_BUCKET=$($env:SCCACHE_BUCKET)"
            "SCCACHE_REGION=$($env:SCCACHE_REGION)"
            "SCCACHE_BASEDIRS=$($env:SCCACHE_BASEDIRS)"
            "AWS_ACCESS_KEY_ID=$($env:AWS_ACCESS_KEY_ID)"
            "AWS_SECRET_ACCESS_KEY=$($env:AWS_SECRET_ACCESS_KEY)"
            "RUSTC_WRAPPER=$($env:RUSTC_WRAPPER)"
        ) | Out-File -FilePath $env:GITHUB_ENV -Append -Encoding utf8
    }

    Write-Information "sccache configured with Cloudflare R2 (bucket: $($env:SCCACHE_BUCKET))" -InformationAction Continue
}

function Show-SccacheConfiguration {
    Write-Output "=== sccache configuration ==="
    Write-Output "sccache version: $(sccache --version)"
    Write-Output "sccache path: $((Get-Command sccache).Source)"
    Write-Output "RUSTC_WRAPPER: $($env:RUSTC_WRAPPER ?? '<not set>')"
    Write-Output "SCCACHE_BUCKET: $($env:SCCACHE_BUCKET ?? '<not set>')"
    Write-Output "SCCACHE_ENDPOINT: $($env:SCCACHE_ENDPOINT ?? '<not set>')"
    Write-Output "SCCACHE_REGION: $($env:SCCACHE_REGION ?? '<not set>')"
    Write-Output "SCCACHE_BASEDIRS: $($env:SCCACHE_BASEDIRS ?? '<not set>')"

    if ($env:AWS_ACCESS_KEY_ID) {
        Write-Output "AWS_ACCESS_KEY_ID: <set>"
    }
    else {
        Write-Output "AWS_ACCESS_KEY_ID: <not set>"
    }

    if ($env:AWS_SECRET_ACCESS_KEY) {
        Write-Output "AWS_SECRET_ACCESS_KEY: <set>"
    }
    else {
        Write-Output "AWS_SECRET_ACCESS_KEY: <not set>"
    }

    Write-Output "=== sccache stats ==="
    sccache --show-stats
}

Install-Sccache
Initialize-SccacheEnvironment
Show-SccacheConfiguration
