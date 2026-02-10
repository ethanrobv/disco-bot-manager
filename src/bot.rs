//! Bot Runtime and Management Module
//!
//! Implements a Supervisor-Worker concurrency model.
//! The BotManager acts as a supervisor, listening for lifecycle commands.
//! The BotInstance is an isolated worker managing a specific Discord connection.

use crate::sources::SourceResolver;
use crate::state::{AccountState, BotCommand, BotStatus, GuildState, SharedState, TrackMetadata};
use serenity::Client;
use serenity::all::{ChannelId, ChannelType, GatewayIntents, GuildId, Http};
use songbird::tracks::PlayMode;
use songbird::{Event, EventContext, EventHandler, SerenityInit, TrackEvent};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, MutexGuard};
use std::time::Duration;
use tokio::sync::mpsc::{self, Receiver};

/// Lifecycle commands sent to the Global Manager.
#[derive(Debug)]
pub enum ManagerCommand {
    /// Spawns a new bot instance for the given account UUID.
    StartBot { uuid: String },
    /// Shuts down the bot instance for the given UUID.
    StopBot { uuid: String },
}

/// The Supervisor that manages the lifecycle of all bot threads.
pub struct BotManager {
    state: SharedState,
    cmd_rx: Receiver<ManagerCommand>,
}

impl BotManager {
    /// Creates a new Manager instance.
    pub fn new(state: SharedState, cmd_rx: Receiver<ManagerCommand>) -> Self {
        Self { state, cmd_rx }
    }

    /// Starts the supervisor loop.
    pub async fn run(mut self) {
        while let Some(cmd) = self.cmd_rx.recv().await {
            match cmd {
                ManagerCommand::StartBot { uuid } => self.spawn_bot(uuid).await,
                ManagerCommand::StopBot { uuid } => self.kill_bot(uuid).await,
            }
        }
    }

    /// Spawns a dedicated Tokio task for a specific bot account.
    async fn spawn_bot(&self, uuid: String) {
        let (token, should_spawn) = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(account) = state.accounts.get_mut(&uuid) {
                if matches!(account.status, BotStatus::Online | BotStatus::Starting) {
                    return;
                }
                account.status = BotStatus::Starting;
                (account.token.clone(), true)
            } else {
                (String::new(), false)
            }
        };

        if !should_spawn {
            return;
        }

        let (tx, rx) = mpsc::channel(32);

        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(account) = state.accounts.get_mut(&uuid) {
                account.command_tx = Some(tx);
            }
        }

        let state_ref = self.state.clone();
        let uuid_clone = uuid.clone();

        tokio::spawn(async move {
            let mut instance = BotInstance::new(uuid_clone, state_ref, rx);
            instance.run(token).await;
        });
    }

    /// Signals a bot instance to shut down by dropping its command channel.
    async fn kill_bot(&self, uuid: String) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(account) = state.accounts.get_mut(&uuid) {
            account.command_tx = None;
            account.status = BotStatus::Offline;
        }
    }
}

/// Listens for Songbird Track events to update the UI state immediately.
struct TrackObserver {
    uuid: String,
    guild_id: u64,
    state: SharedState,
}

#[async_trait::async_trait]
impl EventHandler for TrackObserver {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(states) = ctx {
            let mut app_state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let mut playback_errors = Vec::new();

            if let Some(account) = app_state.accounts.get_mut(&self.uuid) {
                if let Some(guild) = account.guilds.get_mut(&self.guild_id) {
                    for (state, _) in *states {
                        match &state.playing {
                            PlayMode::Errored(e) => {
                                playback_errors.push(format!("{:?}", e));
                            }
                            PlayMode::End => {
                                guild.now_playing = None;
                                guild.is_playing = false;
                            }
                            _ => {}
                        }
                    }
                }
            }

            for error in playback_errors {
                app_state.system_logs.push(format!(
                    "[{}] Playback Error in guild {}: {}",
                    self.uuid, self.guild_id, error
                ));
            }
        }
        None
    }
}

/// Represents a running instance of a Discord Bot.
struct BotInstance {
    uuid: String,
    state: SharedState,
    cmd_rx: Receiver<BotCommand>,
    resolver: SourceResolver,
    songbird: Option<Arc<songbird::Songbird>>,
    http: Option<Arc<Http>>,
    track_lookup: HashMap<uuid::Uuid, TrackMetadata>,
}

impl BotInstance {
    /// Creates a new BotInstance.
    fn new(uuid: String, state: SharedState, cmd_rx: Receiver<BotCommand>) -> Self {
        Self {
            uuid,
            state,
            cmd_rx,
            resolver: SourceResolver::new(),
            songbird: None,
            http: None,
            track_lookup: HashMap::new(),
        }
    }

    /// Main entry point: Connects to Discord and enters the command loop.
    async fn run(&mut self, token: String) {
        self.log("Initializing Discord client...");

        let manager = songbird::Songbird::serenity();
        self.songbird = Some(manager.clone());
        let intents = GatewayIntents::GUILDS | GatewayIntents::GUILD_VOICE_STATES;

        match Client::builder(&token, intents)
            .register_songbird_with(manager)
            .await
        {
            Ok(client) => {
                self.http = Some(client.http.clone());

                if let Ok(info) = client.http.get_current_application_info().await {
                    let app_id = info.id.get();
                    self.update_account(|acc| {
                        acc.application_id = Some(app_id);
                    });
                } else {
                    self.log(
                        "Failed to fetch Application ID. Invite functionality may be limited.",
                    );
                }

                if let Ok(guilds) = client.http.get_guilds(None, None).await {
                    self.update_account(|acc| {
                        acc.status = BotStatus::Online;
                        for g in guilds {
                            acc.guilds
                                .entry(g.id.get())
                                .or_insert_with(|| GuildState::new(g.id.get(), g.name));
                        }
                    });
                }

                let mut runner = client;
                tokio::spawn(async move {
                    let _ = runner.start().await;
                });

                self.log("Connected and Ready.");
                self.command_loop().await;
            }
            Err(e) => {
                self.log(&format!("Connection Failed: {}", e));
                self.update_account(|acc| acc.status = BotStatus::Error(e.to_string()));
            }
        }
    }

    /// The main event loop handling commands and periodic state sync.
    async fn command_loop(&mut self) {
        let mut interval = tokio::time::interval(Duration::from_millis(500));

        loop {
            tokio::select! {
                cmd_opt = self.cmd_rx.recv() => {
                    match cmd_opt {
                        Some(cmd) => self.handle_command(cmd).await,
                        None => {
                            self.log("Command channel closed. Shutting down.");
                            break;
                        }
                    }
                }
                _ = interval.tick() => {
                    self.sync_state().await;
                }
            }
        }

        self.update_account(|acc| acc.status = BotStatus::Offline);
    }

    /// Dispatches incoming commands to their respective handlers.
    async fn handle_command(&mut self, cmd: BotCommand) {
        match cmd {
            BotCommand::Join {
                guild_id,
                channel_id,
            } => {
                if let Some(sb) = &self.songbird {
                    if let Err(e) = sb
                        .join(GuildId::new(guild_id), ChannelId::new(channel_id))
                        .await
                    {
                        self.log(&format!("Failed to join channel: {}", e));
                    }
                }
            }
            BotCommand::Leave { guild_id } => {
                if let Some(sb) = &self.songbird {
                    if let Err(e) = sb.leave(GuildId::new(guild_id)).await {
                        self.log(&format!("Failed to leave channel: {}", e));
                    }
                    self.update_guild(guild_id, |g| g.channel_id = None);
                }
            }
            BotCommand::Play { guild_id, url } => self.play_track(guild_id, url).await,
            BotCommand::Stop { guild_id } => self.call_control(guild_id, |q| q.stop()),
            BotCommand::Skip { guild_id } => self.call_control(guild_id, |q| {
                let _ = q.skip();
            }),
            BotCommand::Pause { guild_id } => self.call_control(guild_id, |q| {
                let _ = q.pause();
            }),
            BotCommand::Resume { guild_id } => self.call_control(guild_id, |q| {
                let _ = q.resume();
            }),
            BotCommand::Volume { guild_id, volume } => {
                self.call_control(guild_id, move |q| {
                    let _ = q.modify_queue(move |tracks| {
                        for t in tracks {
                            let _ = t.set_volume(volume);
                        }
                    });
                });
                self.update_guild(guild_id, |g| g.volume = volume);
            }
            BotCommand::FetchChannels { guild_id } => self.fetch_channels(guild_id).await,
            BotCommand::RemoveTrack {
                guild_id,
                track_uuid,
            } => self.remove_track(guild_id, track_uuid).await,
            BotCommand::MoveTrack {
                guild_id,
                from_index,
                to_index,
            } => self.move_track(guild_id, from_index, to_index).await,
            BotCommand::ClearQueue { guild_id } => {
                self.call_control(guild_id, |q| {
                    let _ = q.modify_queue(|deque| {
                        if deque.len() > 1 {
                            deque.drain(1..);
                        }
                    });
                });
            }
        }
    }

    /// Resolves and plays a track from a URL.
    ///
    /// This method fetches metadata via the SourceResolver, creates a Songbird Track,
    /// attaches event listeners for UI updates (e.g., track end), and enqueues it.
    async fn play_track(&mut self, guild_id: u64, url: String) {
        let Some(sb) = &self.songbird else { return };

        if let Some(handler_lock) = sb.get(GuildId::new(guild_id)) {
            let mut handler = handler_lock.lock().await;

            match self.resolver.resolve(&url).await {
                Ok(resolved) => {
                    let track = songbird::tracks::Track::from(resolved.source);
                    let handle = handler.enqueue(track).await;

                    let metadata = TrackMetadata {
                        uuid: uuid::Uuid::new_v4().to_string(),
                        title: resolved.title.clone(),
                        artist: None,
                        url: url.clone(),
                        duration_secs: resolved.duration.map(|d| d.as_secs()),
                        thumbnail_url: None,
                        added_by: "User".to_string(),
                    };

                    self.track_lookup.insert(handle.uuid(), metadata);

                    let observer = TrackObserver {
                        uuid: self.uuid.clone(),
                        guild_id,
                        state: self.state.clone(),
                    };
                    let _ = handle.add_event(Event::Track(TrackEvent::End), observer);

                    let observer_err = TrackObserver {
                        uuid: self.uuid.clone(),
                        guild_id,
                        state: self.state.clone(),
                    };
                    let _ = handle.add_event(Event::Track(TrackEvent::Error), observer_err);

                    self.log(&format!("Queued: {}", resolved.title));
                }
                Err(e) => self.log(&format!("Source Error: {}", e)),
            }
        } else {
            self.log("Error: Not connected to a voice channel.");
        }
    }

    /// Removes a specific track from the queue based on its UUID.
    async fn remove_track(&self, guild_id: u64, target_uuid: String) {
        let Some(sb) = &self.songbird else { return };
        if let Some(handler_lock) = sb.get(GuildId::new(guild_id)) {
            let handler = handler_lock.lock().await;
            let queue = handler.queue().current_queue();

            for track in queue {
                if let Some(meta) = self.track_lookup.get(&track.uuid()) {
                    if meta.uuid == target_uuid {
                        let _ = track.stop();
                        self.log("Track removed from queue.");
                        break;
                    }
                }
            }
        }
    }

    /// Moves a track within the queue.
    ///
    /// Indices are 0-based relative to the *visible* queue (excluding the currently playing track).
    async fn move_track(&self, guild_id: u64, from: usize, to: usize) {
        self.call_control(guild_id, move |q| {
            let _ = q.modify_queue(move |deque| {
                if deque.len() <= 1 {
                    return;
                }

                // index 0 is Now Playing
                let offset = 1;
                let real_from = from + offset;
                let real_to = to + offset;

                if real_from < deque.len() && real_to < deque.len() {
                    if let Some(item) = deque.remove(real_from) {
                        deque.insert(real_to, item);
                    }
                }
            });
        });
    }

    /// Fetches the voice channels for a guild via the Discord API.
    ///
    /// This updates the shared state so the UI can populate the channel selector dropdown.
    async fn fetch_channels(&self, guild_id: u64) {
        let Some(http) = &self.http else { return };
        if let Ok(channels) = http.get_channels(GuildId::new(guild_id)).await {
            let voice_chans = channels
                .into_iter()
                .filter(|c| c.kind == ChannelType::Voice)
                .map(|c| crate::state::NameId {
                    id: c.id.get(),
                    name: c.name,
                })
                .collect();

            self.update_guild(guild_id, |g| g.voice_channels = voice_chans);
        }
    }

    /// Helper to execute a closure against a guild's track queue safely.
    fn call_control<F>(&self, guild_id: u64, f: F)
    where
        F: FnOnce(&songbird::tracks::TrackQueue) + Send + 'static,
    {
        if let Some(sb) = &self.songbird {
            if let Some(h) = sb.get(GuildId::new(guild_id)) {
                let h = h.clone();
                tokio::spawn(async move {
                    let handler = h.lock().await;
                    f(handler.queue());
                });
            }
        }
    }

    /// Synchronizes the local Songbird state with the SharedState.
    ///
    /// This runs periodically to update playback position, current track, volume,
    /// and queue order for the UI.
    async fn sync_state(&mut self) {
        let Some(sb) = &self.songbird else { return };

        let active_guilds: Vec<u64> = {
            let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(acc) = state.accounts.get(&self.uuid) {
                acc.guilds.keys().cloned().collect()
            } else {
                return;
            }
        };

        let mut active_uuids = std::collections::HashSet::new();

        for guild_id in active_guilds {
            if let Some(call_lock) = sb.get(GuildId::new(guild_id)) {
                let call = call_lock.lock().await;
                let queue_handler = call.queue();

                let current_track_handle = queue_handler.current();
                let full_queue = queue_handler.current_queue();

                let mut new_queue = VecDeque::new();
                let mut now_playing_meta = None;
                let mut is_playing = false;
                let mut is_paused = false;
                let mut position = 0;
                let mut volume = 1.0;

                if let Some(track) = current_track_handle {
                    active_uuids.insert(track.uuid());
                    if let Ok(info) = track.get_info().await {
                        is_playing = info.playing == PlayMode::Play;
                        is_paused = info.playing == PlayMode::Pause;
                        position = info.position.as_secs();
                        volume = info.volume;

                        if let Some(meta) = self.track_lookup.get(&track.uuid()) {
                            now_playing_meta = Some(meta.clone());
                        }
                    }
                }

                for (i, track) in full_queue.iter().enumerate() {
                    active_uuids.insert(track.uuid());

                    // Skip index 0 in the queue list (Now Playing)
                    if i > 0 {
                        if let Some(meta) = self.track_lookup.get(&track.uuid()) {
                            new_queue.push_back(meta.clone());
                        }
                    }
                }

                let channel_id = call.current_channel().map(|c| c.0.get());

                self.update_guild(guild_id, |g| {
                    g.is_playing = is_playing;
                    g.is_paused = is_paused;
                    g.position_secs = position;
                    g.volume = volume;
                    g.now_playing = now_playing_meta;
                    g.queue = new_queue;
                    g.channel_id = channel_id;
                });
            }
        }

        self.track_lookup.retain(|k, _| active_uuids.contains(k));
    }

    /// Thread-safe helper to update the account state.
    fn update_account<F>(&self, f: F)
    where
        F: FnOnce(&mut AccountState),
    {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(acc) = state.accounts.get_mut(&self.uuid) {
            f(acc);
        }
    }

    /// Thread-safe helper to update a specific guild's state.
    fn update_guild<F>(&self, guild_id: u64, f: F)
    where
        F: FnOnce(&mut GuildState),
    {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(acc) = state.accounts.get_mut(&self.uuid) {
            if let Some(guild) = acc.guilds.get_mut(&guild_id) {
                f(guild);
            }
        }
    }

    /// Acquires a lock on the global state.
    fn lock_state(&self) -> MutexGuard<'_, crate::state::AppState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Logs a message to the shared system logs.
    fn log(&self, msg: &str) {
        let mut s = self.lock_state();
        s.log(&format!("[{}] {}", self.uuid, msg));
    }
}
