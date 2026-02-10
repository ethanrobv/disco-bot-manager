#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bot;
mod config;
mod gui;
mod sources;
mod state;

use crate::bot::{BotManager, ManagerCommand};
use crate::config::ConfigManager;
use eframe::egui;
use std::sync::{Arc, Mutex};
use std::thread;
use tokio::sync::mpsc;

/// Dual-thread architecture:
/// 1. **Main Thread**: Runs the synchronous `eframe` (GUI) event loop.
/// 2. **Supervisor Thread**: Runs the asynchronous `tokio` runtime to manage the `BotManager`.
fn main() -> eframe::Result<()> {
    // Load Configuration & Initialize State
    // Persistence data (credentials, etc.) first, then hydrate the AppState.
    let config = ConfigManager::load();
    let initial_state = ConfigManager::init_state(&config);

    // Wrap state in Arc<Mutex<>> for safe concurrent access between GUI and Bot threads.
    let shared_state = Arc::new(Mutex::new(initial_state));

    // Initialize Supervisor Communication
    // The GUI sends high-level lifecycle commands (Start/Stop Bot) to the Manager.
    let (manager_tx, manager_rx) = mpsc::channel::<ManagerCommand>(32);

    // Spawn the Background Supervisor Thread
    // Clone the Arc reference to pass shared ownership to the background thread.
    let state_for_supervisor = shared_state.clone();
    thread::Builder::new()
        .name("BotSupervisor".into())
        .spawn(move || {
            // Build a dedicated Tokio runtime for the background thread
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Failed to build Tokio runtime");

            // Block on the Manager's run loop
            rt.block_on(async move {
                let manager = BotManager::new(state_for_supervisor, manager_rx);
                manager.run().await;
            });
        })
        .expect("Failed to spawn supervisor thread");

    // Configure GUI Window
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1024.0, 768.0]) // optimized for 3-column layout
            .with_min_inner_size([800.0, 600.0])
            .with_title("Disco Bot Manager"),
        ..Default::default()
    };

    eframe::run_native(
        "Disco Bot Manager",
        options,
        Box::new(|cc| Ok(Box::new(gui::MusicApp::new(cc, manager_tx, shared_state)))),
    )
}
