<#
.SYNOPSIS
    Automated Release Bundler for the Music Bot Application.

.DESCRIPTION
    This script orchestrates the release process by performing the following steps
    1. Cleans previous distribution artifacts
    2. Downloads and configures the required yt dlp binary
    3. Downloads and extracts the required ffmpeg binaries
    4. Compiles the Rust application in release mode
    5. Bundles the executable and dependencies into a final dist directory

.EXAMPLE
    .\scripts\bundle.ps1
#>

$ErrorActionPreference = "Stop"

# Configuration Variables
$ProjectRoot = Resolve-Path "$PSScriptRoot\.."
$DistDir = Join-Path $ProjectRoot "dist"
$BinDir = Join-Path $DistDir "bin"
$TargetDir = Join-Path $ProjectRoot "target\release"
$AppName = "music-bot.exe"

# Dependency URLs
$YtDlpUrl = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe"
$FfmpegUrl = "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip"

# Helper function to log status messages
function Write-Status {
    param([string]$Message)
    Write-Host "[BUILD] $Message" -ForegroundColor Cyan
}

# Helper function to handle dependency retrieval
function Invoke-CheckDependency {
    param([string]$Name, [string]$Path, [string]$Url, [string]$Type)

    if (Test-Path $Path) {
        Write-Status "$Name found at location"
        return
    }

    Write-Status "Downloading $Name"

    if ($Type -eq "Direct") {
        Invoke-WebRequest -Uri $Url -OutFile $Path
    }
    elseif ($Type -eq "Zip") {
        $ZipPath = "$Path.zip"
        Invoke-WebRequest -Uri $Url -OutFile $ZipPath

        Write-Status "Extracting $Name"
        Expand-Archive -Path $ZipPath -DestinationPath $BinDir -Force

        # Locate the inner ffmpeg binary from the extracted folder structure
        $ExtractedRoot = Get-ChildItem -Path $BinDir -Filter "ffmpeg-*-essentials_build" | Select-Object -First 1
        if ($ExtractedRoot) {
            $SourceExe = Join-Path $ExtractedRoot.FullName "bin\ffmpeg.exe"
            Move-Item -Path $SourceExe -Destination $Path -Force
            Remove-Item -Path $ExtractedRoot.FullName -Recurse -Force
        }
        Remove-Item -Path $ZipPath -Force
    }

    if (-not (Test-Path $Path)) {
        Write-Error "Failed to install $Name"
    }
    Write-Status "$Name installed successfully"
}

# Execution Flow

# Step 1 Prepare directories (Clean artifact only, preserve dependencies)
if (-not (Test-Path $DistDir)) {
    New-Item -ItemType Directory -Path $DistDir -Force | Out-Null
}
if (-not (Test-Path $BinDir)) {
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
}

# Remove previous executable to ensure fresh copy
$OldBinary = Join-Path $DistDir $AppName
if (Test-Path $OldBinary) {
    Write-Status "Removing previous binary artifact"
    Remove-Item -Path $OldBinary -Force
}

# Step 2 Acquire Dependencies
Write-Status "Checking dependencies"
Invoke-CheckDependency -Name "yt-dlp" `
                       -Path (Join-Path $BinDir "yt-dlp.exe") `
                       -Url $YtDlpUrl `
                       -Type "Direct"

Invoke-CheckDependency -Name "ffmpeg" `
                       -Path (Join-Path $BinDir "ffmpeg.exe") `
                       -Url $FfmpegUrl `
                       -Type "Zip"

# Step 3 Compile Application
Write-Status "Compiling Rust application in Release Mode"
Push-Location $ProjectRoot
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "Cargo build failed" }
}
finally {
    Pop-Location
}

# Step 4 Bundle Artifacts
Write-Status "Bundling artifacts"
$BuiltBinary = Join-Path $TargetDir $AppName
$FinalBinary = Join-Path $DistDir $AppName

if (-not (Test-Path $BuiltBinary)) {
    throw "Build artifact not found at $BuiltBinary"
}

Copy-Item -Path $BuiltBinary -Destination $FinalBinary
Write-Status "Success Distribution created at $DistDir"
