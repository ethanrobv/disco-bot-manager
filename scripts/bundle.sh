#!/bin/bash

# SYNOPSIS
#   Automated Release Bundler for the Music Bot Application.
#
# DESCRIPTION
#   This script performs the following actions
#   1. Cleans previous distribution artifacts
#   2. Downloads and configures the required yt dlp binary
#   3. Downloads and extracts the required ffmpeg binaries
#   4. Compiles the Rust application in release mode
#   5. Bundles the executable and dependencies into a final dist directory

set -e

# Configuration Variables
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST_DIR="$PROJECT_ROOT/dist"
BIN_DIR="$DIST_DIR/bin"
TARGET_DIR="$PROJECT_ROOT/target/release"
APP_NAME="music-bot"

# Dependency URLs
# Note We use the generic linux binary for yt dlp and static builds for ffmpeg
YTDLP_URL="https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp"
FFMPEG_URL="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz"

# Helper function to log status messages
log_status() {
    echo -e "[BUILD] $1"
}

# Execution Flow

# Step 1 Prepare Directories (Clean artifact only, preserve dependencies)
log_status "Preparing dist directories"
mkdir -p "$BIN_DIR"

# Clean previous app artifact only
if [ -f "$DIST_DIR/$APP_NAME" ]; then
    log_status "Removing previous binary artifact"
    rm "$DIST_DIR/$APP_NAME"
fi

# Step 2 Acquire yt dlp
if [ -f "$BIN_DIR/yt-dlp" ]; then
    log_status "yt dlp found"
else
    log_status "Downloading yt dlp"
    curl -L -o "$BIN_DIR/yt-dlp" "$YTDLP_URL"
    chmod +x "$BIN_DIR/yt-dlp"
fi

# Step 3 Acquire ffmpeg
if [ -f "$BIN_DIR/ffmpeg" ]; then
    log_status "ffmpeg found"
else
    log_status "Downloading ffmpeg"
    curl -L -o "$BIN_DIR/ffmpeg.tar.xz" "$FFMPEG_URL"

    log_status "Extracting ffmpeg"
    tar -xf "$BIN_DIR/ffmpeg.tar.xz" -C "$BIN_DIR"

    # Move the binary from the subfolder to the bin root
    # We find the folder starting with ffmpeg and move the binary out
    find "$BIN_DIR" -name "ffmpeg" -type f -exec mv {} "$BIN_DIR" \;

    # Cleanup archive and extracted folders
    rm "$BIN_DIR/ffmpeg.tar.xz"
    find "$BIN_DIR" -type d -name "ffmpeg-*-static" -exec rm -rf {} +
fi

# Step 4 Compile Application
log_status "Compiling Rust application in Release Mode"
cd "$PROJECT_ROOT"
cargo build --release

# Step 5 Bundle Artifacts
log_status "Bundling artifacts"
if [ ! -f "$TARGET_DIR/$APP_NAME" ]; then
    echo "Error Build artifact not found at $TARGET_DIR/$APP_NAME"
    exit 1
fi

cp "$TARGET_DIR/$APP_NAME" "$DIST_DIR/$APP_NAME"

log_status "Success Distribution created at $DIST_DIR"
