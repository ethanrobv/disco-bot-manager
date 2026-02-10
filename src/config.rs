//! Config Module
//!
//! Handles persisting app configuration.
//! Configuration is stored in a `config.json` file located in the same directory as the executable.

use crate::state::{AccountState, AppState, BotStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

/// A single saved account configuration.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SavedAccount {
    pub uuid: String,
    pub alias: String,
    pub token: String,
    pub auto_start: bool,
}

/// The top-level configuration file structure.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct AppConfig {
    pub accounts: Vec<SavedAccount>,
    pub last_selected_account: Option<String>,
}

/// Manages loading and saving of the application configuration.
pub struct ConfigManager;

impl ConfigManager {
    /// Resolves the config file path relative to the executable location.
    fn get_config_path() -> PathBuf {
        env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("config.json")))
            .unwrap_or_else(|| PathBuf::from("config.json"))
    }

    /// Loads the configuration from disk. Returns default if file is missing or invalid.
    pub fn load() -> AppConfig {
        let path = Self::get_config_path();

        if path.exists() {
            if let Ok(content) = fs::read_to_string(path) {
                match serde_json::from_str::<AppConfig>(&content) {
                    Ok(cfg) => return cfg,
                    Err(e) => eprintln!("Failed to parse config: {}", e),
                }
            }
        }
        AppConfig::default()
    }

    /// Saves the provided configuration to disk.
    pub fn save(config: &AppConfig) -> anyhow::Result<()> {
        let path = Self::get_config_path();
        let content = serde_json::to_string_pretty(config)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Converts the static config into the initial runtime state.
    pub fn init_state(config: &AppConfig) -> AppState {
        let mut state = AppState::default();

        state.ui_context.selected_account_uuid = config.last_selected_account.clone();

        for saved in &config.accounts {
            let account = AccountState {
                uuid: saved.uuid.clone(),
                alias: saved.alias.clone(),
                token: saved.token.clone(),
                application_id: None,
                auto_start: saved.auto_start,
                status: BotStatus::Offline,
                guilds: HashMap::new(),
                command_tx: None,
            };
            state.accounts.insert(saved.uuid.clone(), account);
        }

        state
    }

    /// Updates the config based on the current runtime state.
    pub fn update_from_state(state: &AppState) -> AppConfig {
        let mut accounts: Vec<SavedAccount> = state
            .accounts
            .values()
            .map(|acc| SavedAccount {
                uuid: acc.uuid.clone(),
                alias: acc.alias.clone(),
                token: acc.token.clone(),
                auto_start: acc.auto_start,
            })
            .collect();

        // Sort for consistent file output
        accounts.sort_by(|a, b| a.alias.cmp(&b.alias));

        AppConfig {
            accounts,
            last_selected_account: state.ui_context.selected_account_uuid.clone(),
        }
    }
}
