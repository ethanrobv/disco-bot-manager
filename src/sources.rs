//! Audio Source Resolution Module
//!
//! Handles the parsing of URLs and the creation of playable audio sources.
//! 0. Injects bundled dependencies into path (ffmpeg, etc.).
//! 1. Fetches metadata and stream URL via `yt-dlp`.
//! 2. Streams audio via `ffmpeg` using the direct URL.

use anyhow::{Context, Result, anyhow};
use reqwest::Client;
use serde::Deserialize;
use songbird::input::{ChildContainer, Input};
use std::env;
use std::process::{Command, Stdio};
use std::sync::Once;
use std::time::Duration;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

// Windows flag to suppress console window creation
const CREATE_NO_WINDOW: u32 = 0x08000000;

static INIT_PATH: Once = Once::new();

fn inject_local_binaries() {
    INIT_PATH.call_once(|| {
        if let Ok(path_var) = env::var("PATH") {
            if let Ok(cwd) = env::current_exe() {
                if let Some(parent) = cwd.parent() {
                    let bin_dir = parent.join("bin");
                    if bin_dir.exists() {
                        let new_path = format!("{};{}", bin_dir.display(), path_var);
                        unsafe {
                            env::set_var("PATH", new_path);
                        }
                    }
                }
            }
        }
    });
}

pub struct ResolvedSource {
    pub source: Input,
    pub title: String,
    pub duration: Option<Duration>,
}

#[derive(Deserialize)]
struct YtDlpMetadata {
    title: Option<String>,
    duration: Option<f64>,
    url: Option<String>, // Direct stream URL
}

pub struct SourceResolver {
    _http_client: Client,
}

impl SourceResolver {
    pub fn new() -> Self {
        inject_local_binaries();
        Self {
            _http_client: Client::new(),
        }
    }

    pub async fn resolve(&self, url: &str) -> Result<ResolvedSource> {
        // Fetch Metadata & Stream URL
        let metadata = self.fetch_metadata(url).await?;

        // Create Audio Stream via FFmpeg
        let mut cmd = if let Some(stream_url) = &metadata.url {
            let mut c = Command::new("ffmpeg");
            c.args([
                "-reconnect",
                "1",
                "-reconnect_streamed",
                "1",
                "-reconnect_delay_max",
                "5",
                "-i",
                stream_url,
                "-f",
                "wav", // Output as WAV (header + PCM) for easy probing
                "-ar",
                "48000", // Standard sample rate
                "-ac",
                "2", // Stereo
                "-map",
                "a", // Map audio only
                "-", // Output to stdout
            ]);
            c
        } else {
            // Fallback: If no direct URL, let yt-dlp pipe it
            let mut c = Command::new("yt-dlp");
            c.args([
                "-f",
                "bestaudio/best",
                "-o",
                "-",
                "-q",
                "--no-warnings",
                url,
            ]);
            c
        };

        // Suppress Window on Windows
        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);

        // Pipe Output
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());

        let child = cmd
            .spawn()
            .context("Failed to spawn audio stream process (ffmpeg/yt-dlp)")?;

        // Wrap in Songbird Input
        let source = Input::from(ChildContainer::from(child));

        Ok(ResolvedSource {
            source,
            title: metadata
                .title
                .unwrap_or_else(|| "Unknown Title".to_string()),
            duration: metadata.duration.map(Duration::from_secs_f64),
        })
    }

    async fn fetch_metadata(&self, url: &str) -> Result<YtDlpMetadata> {
        let mut cmd = tokio::process::Command::new("yt-dlp");
        cmd.args([
            "--dump-json",   // JSON Output
            "--no-playlist", // Single track only
            "-f",
            "bestaudio/best", // Select best audio format to get correct URL
            "-q",
            url,
        ]);

        #[cfg(target_os = "windows")]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let output = cmd
            .output()
            .await
            .context("Failed to execute yt-dlp for metadata")?;

        if !output.status.success() {
            return Err(anyhow!("yt-dlp failed to fetch metadata"));
        }

        let json_str = String::from_utf8(output.stdout).context("Invalid UTF-8 in metadata")?;
        let metadata: YtDlpMetadata =
            serde_json::from_str(&json_str).context("Failed to parse yt-dlp JSON")?;

        Ok(metadata)
    }
}
