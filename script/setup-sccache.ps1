#Requires -Version 5.1
$ErrorActionPreference = "Stop"

$SCCACHE_VERSION = "v0.14.0"
$SCCACHE_DIR = "./target/sccache"

function Install-Sccache {
    New-Item -ItemType Directory -Path $SCCACHE_DIR -Force | Out-Null

    $sccachePath = Join-Path $SCCACHE_DIR "sccache.exe"

    if (Test-Path $sccachePath) {
        Write-Host "sccache already cached: $(& $sccachePath --version)"
    }
    else {
        Write-Host "Installing sccache ${SCCACHE_VERSION} from GitHub releases..."

        if (-not [Environment]::Is64BitOperatingSystem) {
            Write-Host "Error: 64-bit Windows is required"
            exit 1
        }
        $arch = "x86_64"
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

            Write-Host "Installed sccache: $(& $sccachePath --version)"
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
        Write-Host "::error::sccache was installed but is not found in PATH"
        Write-Host "PATH: $env:PATH"
        Write-Host "Expected location: $absolutePath"
        if (Test-Path (Join-Path $absolutePath "sccache.exe")) {
            Write-Host "sccache.exe exists at expected location but is not in PATH"
            Write-Host "Directory contents:"
            Get-ChildItem $absolutePath | ForEach-Object { Write-Host "  $_" }
        }
        else {
            Write-Host "sccache.exe NOT found at expected location"
        }
        exit 1
    }
}

function Check-MissingR2Configuration {
    $missing = @()

    foreach ($name in @("R2_ACCOUNT_ID", "R2_ACCESS_KEY_ID", "R2_SECRET_ACCESS_KEY", "R2_SCCACHE_BUCKET")) {
        if (-not [Environment]::GetEnvironmentVariable($name)) {
            $missing += $name
        }
    }

    if ($missing.Length -gt 0) {
        Write-Host "Missing $($missing -join ', '), skipping sccache configuration"
        return $true
    }

    return $false
}

function Configure-Sccache {
    if (Check-MissingR2Configuration) {
        return
    }

    $sccacheCommand = Get-Command sccache -ErrorAction SilentlyContinue
    if (-not $sccacheCommand) {
        Write-Host "::error::sccache not found in PATH, cannot configure RUSTC_WRAPPER"
        Write-Host "PATH: $env:PATH"
        exit 1
    }

    Write-Host "Configuring sccache with Cloudflare R2..."

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

    Write-Host "sccache configured with Cloudflare R2 (bucket: $($env:SCCACHE_BUCKET))"
}

function Format-EnvValue {
    param (
        [string]$Value
    )

    if ($Value) {
        return $Value
    }

    return "<not set>"
}

function Show-Config {
    Write-Host "=== sccache configuration ==="
    Write-Host "sccache version: $(sccache --version)"
    Write-Host "sccache path: $((Get-Command sccache).Source)"
    Write-Host "RUSTC_WRAPPER: $(Format-EnvValue $env:RUSTC_WRAPPER)"
    Write-Host "SCCACHE_BUCKET: $(Format-EnvValue $env:SCCACHE_BUCKET)"
    Write-Host "SCCACHE_ENDPOINT: $(Format-EnvValue $env:SCCACHE_ENDPOINT)"
    Write-Host "SCCACHE_REGION: $(Format-EnvValue $env:SCCACHE_REGION)"
    Write-Host "SCCACHE_BASEDIRS: $(Format-EnvValue $env:SCCACHE_BASEDIRS)"

    if ($env:AWS_ACCESS_KEY_ID) {
        Write-Host "AWS_ACCESS_KEY_ID: <set>"
    }
    else {
        Write-Host "AWS_ACCESS_KEY_ID: <not set>"
    }

    if ($env:AWS_SECRET_ACCESS_KEY) {
        Write-Host "AWS_SECRET_ACCESS_KEY: <set>"
    }
    else {
        Write-Host "AWS_SECRET_ACCESS_KEY: <not set>"
    }

    Write-Host "=== sccache stats ==="
    sccache --show-stats
}

Install-Sccache
Configure-Sccache
Show-Config
