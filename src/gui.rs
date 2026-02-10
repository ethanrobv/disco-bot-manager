//! GUI Module
//!
//! - High-contrast dark theme
//! - Panels style UI

use crate::bot::ManagerCommand;
use crate::config::ConfigManager;
use crate::state::{AccountState, AppState, BotCommand, BotStatus, GuildState, SharedState};
use eframe::egui;
use egui::{Color32, FontFamily, FontId, Key, RichText, Stroke, TextStyle};
use egui_extras::{Column, TableBuilder};
use tokio::sync::mpsc::Sender;

/// Main application state struct for the GUI.
pub struct MusicApp {
    state: SharedState,
    manager_tx: Sender<ManagerCommand>,
    add_account_token: String,
    add_account_alias: String,
    show_add_modal: bool,
    url_input: String,
}

impl MusicApp {
    /// Creates a new MusicApp instance and configures the UI theme.
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        manager_tx: Sender<ManagerCommand>,
        state: SharedState,
    ) -> Self {
        Self::configure_style(&_cc.egui_ctx);

        Self {
            state,
            manager_tx,
            add_account_token: String::new(),
            add_account_alias: String::new(),
            show_add_modal: false,
            url_input: String::new(),
        }
    }

    fn configure_style(ctx: &egui::Context) {
        let mut style = (*ctx.style()).clone();

        style.text_styles = [
            (TextStyle::Heading, FontId::new(20.0, FontFamily::Monospace)),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Monospace)),
            (
                TextStyle::Monospace,
                FontId::new(14.0, FontFamily::Monospace),
            ),
            (TextStyle::Button, FontId::new(14.0, FontFamily::Monospace)),
            (TextStyle::Small, FontId::new(10.0, FontFamily::Monospace)),
        ]
        .into();

        let mut visuals = egui::Visuals::dark();
        visuals.window_fill = Color32::from_rgb(30, 30, 30);
        visuals.panel_fill = Color32::from_rgb(30, 30, 30);

        visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(40, 40, 40);
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(45, 45, 45);
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(60, 60, 60);
        visuals.widgets.active.bg_fill = Color32::from_rgb(80, 80, 80);

        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(60, 60, 60));
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(60, 60, 60));

        visuals.selection.bg_fill = Color32::from_rgb(50, 100, 160);

        ctx.set_style(style);
        ctx.set_visuals(visuals);
    }
}

impl eframe::App for MusicApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());

        Self::render_logs_panel(ctx, &state);

        Self::render_accounts_panel(ctx, &mut state, &self.manager_tx, &mut self.show_add_modal);

        Self::render_guilds_panel(ctx, &mut state, &self.manager_tx);

        Self::render_dashboard(ctx, &mut state, &mut self.url_input);

        if self.show_add_modal {
            Self::render_add_account_modal(
                ctx,
                &mut state,
                &self.manager_tx,
                &mut self.add_account_token,
                &mut self.add_account_alias,
                &mut self.show_add_modal,
            );
        }

        ctx.request_repaint();
    }
}

impl MusicApp {
    /// Renders the bottom panel containing system logs.
    fn render_logs_panel(ctx: &egui::Context, state: &AppState) {
        egui::TopBottomPanel::bottom("log_panel")
            .resizable(true)
            .min_height(100.0)
            .default_height(150.0)
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(Color32::TRANSPARENT)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.heading("System Logs");
                        ui.separator();

                        egui::ScrollArea::both()
                            .stick_to_bottom(true)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for log in &state.system_logs {
                                    ui.add(egui::Label::new(
                                        RichText::new(log).font(FontId::monospace(12.0)),
                                    ));
                                }
                            });
                    });
            });
    }

    /// Renders the left-most panel containing the list of configured bot accounts.
    fn render_accounts_panel(
        ctx: &egui::Context,
        state: &mut AppState,
        manager_tx: &Sender<ManagerCommand>,
        show_add_modal: &mut bool,
    ) {
        egui::SidePanel::left("accounts_panel")
            .exact_width(220.0)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(Color32::TRANSPARENT)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.heading("ACCOUNTS");
                        ui.add_space(5.0);
                        ui.separator();
                        ui.add_space(10.0);

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            ui.with_layout(
                                egui::Layout::top_down_justified(egui::Align::LEFT),
                                |ui| {
                                    let mut accounts: Vec<_> =
                                        state.accounts.values().cloned().collect();
                                    accounts.sort_by(|a, b| a.alias.cmp(&b.alias));

                                    for account in accounts {
                                        Self::render_account_item(
                                            ctx, ui, state, &account, manager_tx,
                                        );
                                    }
                                },
                            );
                        });

                        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                            if ui.button(" + Add Account ").clicked() {
                                *show_add_modal = true;
                            }
                            ui.separator();
                        });
                    });
            });
    }

    /// Renders a single account row in the accounts panel.
    fn render_account_item(
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        state: &mut AppState,
        account: &AccountState,
        manager_tx: &Sender<ManagerCommand>,
    ) {
        let is_selected = state.ui_context.selected_account_uuid.as_deref() == Some(&account.uuid);

        let status_color = match account.status {
            BotStatus::Online => Color32::GREEN,
            BotStatus::Starting => Color32::YELLOW,
            BotStatus::Offline => Color32::GRAY,
            BotStatus::Error(_) => Color32::RED,
        };

        let resp = ui
            .horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().circle_filled(rect.center(), 4.0, status_color);

                ui.selectable_label(is_selected, &account.alias)
            })
            .inner;

        if resp.clicked() {
            state.ui_context.selected_account_uuid = Some(account.uuid.clone());
            state.ui_context.selected_guild_id = None;
        }

        resp.context_menu(|ui| {
            ui.label(format!("Status: {:?}", account.status));
            ui.separator();

            if let Some(app_id) = account.application_id {
                if ui.button("Invite to Server").clicked() {
                    let url = format!("https://discord.com/api/oauth2/authorize?client_id={}&permissions=36700160&scope=bot", app_id);
                    ctx.open_url(egui::OpenUrl::new_tab(url));
                }
            } else {
                ui.label(RichText::new("Invite unavailable (No ID)").color(Color32::GRAY));
            }
            ui.separator();

            if matches!(account.status, BotStatus::Offline) {
                if ui.button("Start Bot").clicked() {
                    let _ = manager_tx.try_send(ManagerCommand::StartBot {
                        uuid: account.uuid.clone(),
                    });
                }
            } else {
                if ui.button("Stop Bot").clicked() {
                    let _ = manager_tx.try_send(ManagerCommand::StopBot {
                        uuid: account.uuid.clone(),
                    });
                }
            }
            if ui.button("Delete").clicked() {
                state.accounts.remove(&account.uuid);
                let cfg = ConfigManager::update_from_state(state);
                let _ = ConfigManager::save(&cfg);
            }
        });
    }

    /// Renders the middle panel containing the list of guilds for the selected account.
    fn render_guilds_panel(
        ctx: &egui::Context,
        state: &mut AppState,
        manager_tx: &Sender<ManagerCommand>,
    ) {
        if state.ui_context.selected_account_uuid.is_none() {
            return;
        }

        egui::SidePanel::left("guilds_panel")
            .exact_width(240.0)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(Color32::TRANSPARENT)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.heading("SERVERS");
                        ui.add_space(5.0);
                        ui.separator();
                        ui.add_space(10.0);

                        let uuid = state
                            .ui_context
                            .selected_account_uuid
                            .clone()
                            .unwrap_or_else(|| "".to_string());

                        if let Some(account) = state.accounts.get_mut(&uuid) {
                            if matches!(account.status, BotStatus::Offline) {
                                ui.centered_and_justified(|ui| {
                                    ui.vertical_centered(|ui| {
                                        ui.label(
                                            RichText::new("Bot is Offline").color(Color32::GRAY),
                                        );
                                        ui.add_space(10.0);
                                        if ui.button("Start Bot").clicked() {
                                            let _ = manager_tx.try_send(ManagerCommand::StartBot {
                                                uuid: account.uuid.clone(),
                                            });
                                        }
                                    });
                                });
                            } else {
                                let cmd_tx = account.command_tx.clone();

                                if account.guilds.is_empty() {
                                    ui.label("No servers detected.");
                                } else {
                                    egui::ScrollArea::vertical().show(ui, |ui| {
                                        let mut guilds: Vec<_> = account.guilds.values().collect();
                                        guilds.sort_by(|a, b| a.guild_name.cmp(&b.guild_name));

                                        for guild in guilds {
                                            let is_active = state.ui_context.selected_guild_id
                                                == Some(guild.guild_id);
                                            let label = if guild.is_playing {
                                                format!("(Playing) {}", guild.guild_name)
                                            } else {
                                                guild.guild_name.clone()
                                            };

                                            if ui.selectable_label(is_active, label).clicked() {
                                                state.ui_context.selected_guild_id =
                                                    Some(guild.guild_id);
                                                if let Some(tx) = &cmd_tx {
                                                    let _ =
                                                        tx.try_send(BotCommand::FetchChannels {
                                                            guild_id: guild.guild_id,
                                                        });
                                                }
                                            }
                                        }
                                    });
                                }
                            }
                        }
                    });
            });
    }

    /// Renders the main dashboard area with player controls and the track queue.
    fn render_dashboard(ctx: &egui::Context, state: &mut AppState, url_input: &mut String) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::Frame::default()
                .fill(Color32::TRANSPARENT)
                .inner_margin(15.0)
                .show(ui, |ui| {
                    let uuid_opt = state.ui_context.selected_account_uuid.clone();
                    let gid_opt = state.ui_context.selected_guild_id;

                    if let (Some(uuid), Some(gid)) = (uuid_opt, gid_opt) {
                        let cmd_tx_opt =
                            state.accounts.get(&uuid).and_then(|a| a.command_tx.clone());

                        if let Some(account) = state.accounts.get_mut(&uuid) {
                            if let Some(guild) = account.guilds.get_mut(&gid) {
                                Self::render_header(ui, &cmd_tx_opt, guild);
                                ui.add_space(15.0);

                                if guild.channel_id.is_some() {
                                    Self::render_player_box(ui, &cmd_tx_opt, guild, url_input);
                                    ui.add_space(15.0);
                                    Self::render_queue_table(ui, &cmd_tx_opt, guild);
                                } else {
                                    ui.centered_and_justified(|ui| {
                                        ui.label(
                                            RichText::new(
                                                "Join a voice channel to enable playback controls.",
                                            )
                                            .color(Color32::GRAY),
                                        );
                                    });
                                }
                                return;
                            }
                        }
                    }

                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Select a server to manage audio.")
                                .size(16.0)
                                .color(Color32::GRAY),
                        );
                    });
                });
        });
    }

    /// Renders the header of the dashboard, including connection controls and channel selection.
    fn render_header(ui: &mut egui::Ui, tx: &Option<Sender<BotCommand>>, guild: &GuildState) {
        ui.heading(RichText::new(&guild.guild_name).size(24.0).strong());
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            if ui.button("Refresh").clicked() {
                if let Some(t) = tx {
                    let _ = t.try_send(BotCommand::FetchChannels {
                        guild_id: guild.guild_id,
                    });
                }
            }

            let current = guild
                .channel_id
                .and_then(|id| guild.voice_channels.iter().find(|c| c.id == id))
                .map(|c| c.name.clone())
                .unwrap_or_else(|| "Select Channel...".to_string());

            egui::ComboBox::from_id_salt("chan_sel")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for c in &guild.voice_channels {
                        if ui
                            .selectable_label(Some(c.id) == guild.channel_id, &c.name)
                            .clicked()
                        {
                            if let Some(t) = tx {
                                let _ = t.try_send(BotCommand::Join {
                                    guild_id: guild.guild_id,
                                    channel_id: c.id,
                                });
                            }
                        }
                    }
                });

            if guild.channel_id.is_some() {
                if ui.button("Disconnect").clicked() {
                    if let Some(t) = tx {
                        let _ = t.try_send(BotCommand::Leave {
                            guild_id: guild.guild_id,
                        });
                    }
                }
            }
        });
    }

    /// Renders the audio player controls.
    fn render_player_box(
        ui: &mut egui::Ui,
        tx: &Option<Sender<BotCommand>>,
        guild: &mut GuildState,
        url: &mut String,
    ) {
        egui::Frame::group(ui.style())
            .inner_margin(15.0)
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    let title = guild
                        .now_playing
                        .as_ref()
                        .map(|t| t.title.as_str())
                        .unwrap_or("No Active Track");
                    ui.label(RichText::new(title).size(18.0).color(Color32::WHITE));
                    ui.add_space(5.0);

                    let dur = guild
                        .now_playing
                        .as_ref()
                        .and_then(|t| t.duration_secs)
                        .unwrap_or(1)
                        .max(1);
                    let pct = guild.position_secs as f32 / dur as f32;
                    let text = format!(
                        "{:02}:{:02} / {:02}:{:02}",
                        guild.position_secs / 60,
                        guild.position_secs % 60,
                        dur / 60,
                        dur % 60
                    );

                    let bar = egui::ProgressBar::new(pct)
                        .text(text)
                        .animate(guild.is_playing);
                    ui.add_sized([400.0, 20.0], bar);
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        ui.label("Vol");
                        let mut vol = guild.volume;
                        if ui
                            .add(egui::Slider::new(&mut vol, 0.0..=1.0).show_value(false))
                            .drag_stopped()
                        {
                            if let Some(t) = tx {
                                let _ = t.try_send(BotCommand::Volume {
                                    guild_id: guild.guild_id,
                                    volume: vol,
                                });
                            }
                        }

                        ui.separator();

                        if ui
                            .add_sized([60.0, 20.0], egui::Button::new("Stop"))
                            .clicked()
                        {
                            if let Some(t) = tx {
                                let _ = t.try_send(BotCommand::Stop {
                                    guild_id: guild.guild_id,
                                });
                            }
                        }

                        let icon = if guild.is_paused { "Resume" } else { "Pause" };
                        if ui
                            .add_sized([60.0, 20.0], egui::Button::new(icon))
                            .clicked()
                        {
                            if let Some(t) = tx {
                                let c = if guild.is_paused {
                                    BotCommand::Resume {
                                        guild_id: guild.guild_id,
                                    }
                                } else {
                                    BotCommand::Pause {
                                        guild_id: guild.guild_id,
                                    }
                                };
                                let _ = t.try_send(c);
                            }
                        }

                        if ui
                            .add_sized([60.0, 20.0], egui::Button::new("Skip"))
                            .clicked()
                        {
                            if let Some(t) = tx {
                                let _ = t.try_send(BotCommand::Skip {
                                    guild_id: guild.guild_id,
                                });
                            }
                        }
                    });
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.label("Add Track:");

                    let btn_width = 80.0;
                    let input_width = ui.available_width() - btn_width - 10.0;

                    let response = ui.add(
                        egui::TextEdit::singleline(url)
                            .desired_width(input_width)
                            .hint_text("Paste YouTube URL here..."),
                    );

                    let clicked_add = ui.add(egui::Button::new("Enqueue")).clicked();

                    let enter_pressed =
                        response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));

                    if (clicked_add || enter_pressed) && !url.is_empty() {
                        if let Some(t) = tx {
                            let _ = t.try_send(BotCommand::Play {
                                guild_id: guild.guild_id,
                                url: url.clone(),
                            });
                            url.clear();
                            response.request_focus();
                        }
                    }
                });
            });
    }

    /// Renders the track queue table.
    fn render_queue_table(ui: &mut egui::Ui, tx: &Option<Sender<BotCommand>>, guild: &GuildState) {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.heading("Queue");
                ui.label(RichText::new(format!("({} tracks)", guild.queue.len())).weak());

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !guild.queue.is_empty() {
                        if ui.button("Clear").clicked() {
                            if let Some(t) = tx {
                                let _ = t.try_send(BotCommand::ClearQueue {
                                    guild_id: guild.guild_id,
                                });
                            }
                        }
                    }
                });
            });

            ui.separator();

            egui::ScrollArea::vertical()
                .max_height(300.0)
                .show(ui, |ui| {
                    TableBuilder::new(ui)
                        .striped(true)
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .column(Column::exact(30.0))
                        .column(Column::remainder())
                        .column(Column::exact(60.0))
                        .column(Column::exact(140.0))
                        .header(20.0, |mut header| {
                            header.col(|ui| {
                                ui.label("#");
                            });
                            header.col(|ui| {
                                ui.label("Title");
                            });
                            header.col(|ui| {
                                ui.label("Time");
                            });
                            header.col(|ui| {
                                ui.label("Actions");
                            });
                        })
                        .body(|mut body| {
                            for (i, track) in guild.queue.iter().enumerate() {
                                body.row(24.0, |mut row| {
                                    row.col(|ui| {
                                        ui.label((i + 1).to_string());
                                    });
                                    row.col(|ui| {
                                        ui.add(
                                            egui::Label::new(RichText::new(&track.title).strong())
                                                .truncate(),
                                        );
                                    });
                                    row.col(|ui| {
                                        let s = track.duration_secs.unwrap_or(0);
                                        ui.label(format!("{:02}:{:02}", s / 60, s % 60));
                                    });
                                    row.col(|ui| {
                                        ui.horizontal(|ui| {
                                            if ui
                                                .add_enabled(i > 0, egui::Button::new("Up").small())
                                                .clicked()
                                            {
                                                if let Some(t) = tx {
                                                    let _ = t.try_send(BotCommand::MoveTrack {
                                                        guild_id: guild.guild_id,
                                                        from_index: i,
                                                        to_index: i - 1,
                                                    });
                                                }
                                            }

                                            if ui
                                                .add_enabled(
                                                    i < guild.queue.len() - 1,
                                                    egui::Button::new("Down").small(),
                                                )
                                                .clicked()
                                            {
                                                if let Some(t) = tx {
                                                    let _ = t.try_send(BotCommand::MoveTrack {
                                                        guild_id: guild.guild_id,
                                                        from_index: i,
                                                        to_index: i + 1,
                                                    });
                                                }
                                            }

                                            if ui.small_button("Del").clicked() {
                                                if let Some(t) = tx {
                                                    let _ = t.try_send(BotCommand::RemoveTrack {
                                                        guild_id: guild.guild_id,
                                                        track_uuid: track.uuid.clone(),
                                                    });
                                                }
                                            }
                                        });
                                    });
                                });
                            }
                        });
                });
        });
    }

    /// Renders the modal dialog for adding a new bot account.
    fn render_add_account_modal(
        ctx: &egui::Context,
        state: &mut AppState,
        manager_tx: &Sender<ManagerCommand>,
        token: &mut String,
        alias: &mut String,
        show: &mut bool,
    ) {
        egui::Window::new("Add Bot Account")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(300.0);
                ui.label("Bot Token:");
                ui.add(
                    egui::TextEdit::singleline(token)
                        .password(true)
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(5.0);
                ui.label("Alias:");
                ui.add(egui::TextEdit::singleline(alias).desired_width(f32::INFINITY));
                ui.add_space(15.0);

                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        *show = false;
                    }
                    if ui.button("Connect").clicked() {
                        if !token.is_empty() && !alias.is_empty() {
                            let uuid = uuid::Uuid::new_v4().to_string();
                            let acc = AccountState::new(uuid.clone(), alias.clone(), token.clone());

                            state.accounts.insert(uuid.clone(), acc);
                            let cfg = ConfigManager::update_from_state(state);
                            let _ = ConfigManager::save(&cfg);

                            let _ = manager_tx.try_send(ManagerCommand::StartBot { uuid });

                            *show = false;
                            token.clear();
                            alias.clear();
                        }
                    }
                });
            });
    }
}
