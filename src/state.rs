//! State Module
//!
//! Handles global application state.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::Sender;

/// Commands sent from the UI to a Bot Instance.
#[derive(Debug, Clone)]
pub enum BotCommand {
    /// Connect to a specific voice channel in a guild.
    Join { guild_id: u64, channel_id: u64 },
    /// Disconnect from the voice channel in a guild.
    Leave { guild_id: u64 },

    /// Enqueue a track from a URL.
    Play { guild_id: u64, url: String },
    /// Pause playback.
    Pause { guild_id: u64 },
    /// Resume playback.
    Resume { guild_id: u64 },
    /// Stop playback and clear the queue.
    Stop { guild_id: u64 },
    /// Skip the current track.
    Skip { guild_id: u64 },
    /// Set the volume (0.0 to 1.0).
    Volume { guild_id: u64, volume: f32 },

    /// Remove a specific track from the queue by its UUID.
    RemoveTrack { guild_id: u64, track_uuid: String },
    /// Moves a track from one index to another in the queue.
    MoveTrack {
        guild_id: u64,
        from_index: usize,
        to_index: usize,
    },
    /// Clear all upcoming tracks from the queue.
    ClearQueue { guild_id: u64 },

    /// Refresh the list of available voice channels for a guild.
    FetchChannels { guild_id: u64 },
}

/// Represents a named entity with an ID (e.g., Guild or Channel).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NameId {
    pub id: u64,
    pub name: String,
}

/// Metadata for a single audio track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackMetadata {
    /// Unique identifier for UI operations (removal/reordering).
    pub uuid: String,
    pub title: String,
    pub artist: Option<String>,
    pub url: String,
    pub duration_secs: Option<u64>,
    pub thumbnail_url: Option<String>,
    pub added_by: String,
}

/// The runtime status of a bot instance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BotStatus {
    Offline,
    Starting,
    Online,
    Error(String),
}

/// Persists the state of a specific guild (queue, volume, channel).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuildState {
    pub guild_id: u64,
    pub guild_name: String,
    pub channel_id: Option<u64>,

    pub is_playing: bool,
    pub is_paused: bool,
    pub volume: f32,
    pub position_secs: u64,

    pub now_playing: Option<TrackMetadata>,
    pub queue: VecDeque<TrackMetadata>,

    pub voice_channels: Vec<NameId>,
}

impl GuildState {
    /// Creates a new, empty guild state.
    pub fn new(id: u64, name: String) -> Self {
        Self {
            guild_id: id,
            guild_name: name,
            channel_id: None,
            is_playing: false,
            is_paused: false,
            volume: 1.0,
            position_secs: 0,
            now_playing: None,
            queue: VecDeque::new(),
            voice_channels: Vec::new(),
        }
    }
}

/// Represents a single Bot Token/Instance and its associated data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountState {
    pub uuid: String,
    pub alias: String,
    pub token: String,

    /// The Discord Application ID (Client ID) used for generating invite links.
    pub application_id: Option<u64>,

    /// Determines if this bot should start automatically when the app launches.
    pub auto_start: bool,

    pub status: BotStatus,
    pub guilds: HashMap<u64, GuildState>,

    #[serde(skip)]
    pub command_tx: Option<Sender<BotCommand>>,
}

impl AccountState {
    /// Creates a new account state with offline status.
    /// Defaults `auto_start` to true for new accounts.
    pub fn new(uuid: String, alias: String, token: String) -> Self {
        Self {
            uuid,
            alias,
            token,
            application_id: None,
            auto_start: true,
            status: BotStatus::Offline,
            guilds: HashMap::new(),
            command_tx: None,
        }
    }
}

/// Stores UI-specific context for persistence (e.g., selected tab).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiContext {
    pub selected_account_uuid: Option<String>,
    pub selected_guild_id: Option<u64>,
}

/// The global source of truth for the application state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    pub accounts: HashMap<String, AccountState>,
    pub ui_context: UiContext,
    pub system_logs: Vec<String>,
}

pub type SharedState = Arc<Mutex<AppState>>;

impl AppState {
    /// Appends a log message to the system logs with a timestamp.
    pub fn log(&mut self, msg: &str) {
        let timestamp = chrono::Local::now().format("%H:%M:%S");
        self.system_logs.push(format!("[{}] {}", timestamp, msg));
        if self.system_logs.len() > 1000 {
            self.system_logs.remove(0);
        }
    }
}
