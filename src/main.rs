mod auth;
mod config;
mod email;
mod ui;
mod reminders;

use anyhow::Result;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

use auth::GoogleAuth;
use config::Config;
use email::ImapClient;
use ui::{handle_key_event, App};
use reminders::RemindersFile;

fn main() -> Result<()> {
    let mut config = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    let access_token = if let Some(ref token) = config.access_token {
        token.clone()
    } else {
        println!("No access token found. Starting OAuth flow...");
        let auth = GoogleAuth::new(&config)?;
        let (access_token, refresh_token) = auth.authenticate()?;
        config.access_token = Some(access_token.clone());
        config.refresh_token = Some(refresh_token);
        config.save()?;
        println!("Authentication successful!");
        access_token
    };

    println!("Connecting to Gmail...");
    let mut imap_client = match ImapClient::connect(&config.email, &access_token) {
        Ok(client) => client,
        Err(_) => {
            if let Some(ref refresh_token) = config.refresh_token {
                println!("Access token expired, refreshing...");
                let auth = GoogleAuth::new(&config)?;
                let new_token = auth.refresh_token(refresh_token)?;
                config.access_token = Some(new_token.clone());
                config.save()?;
                ImapClient::connect(&config.email, &new_token)?
            } else {
                println!("Re-authenticating...");
                let auth = GoogleAuth::new(&config)?;
                let (access_token, refresh_token) = auth.authenticate()?;
                config.access_token = Some(access_token.clone());
                config.refresh_token = Some(refresh_token);
                config.save()?;
                ImapClient::connect(&config.email, &access_token)?
            }
        }
    };

    println!("Fetching emails...");
    let emails = imap_client.fetch_emails("INBOX", 0, 50)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    app.set_emails(emails);

    let mut reminders = RemindersFile::load().unwrap_or_default();
    let result = run_app(&mut terminal, &mut app, &mut imap_client, &config, &mut reminders);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    let _ = imap_client.logout();

    result
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    imap_client: &mut ImapClient,
    config: &Config,
    reminders: &mut RemindersFile,
) -> Result<()> {
    loop {
        let view_height = terminal.size()?.height.saturating_sub(8);

        terminal.draw(|f| app.render(f))?;

        if let Event::Key(key) = event::read()? {
            let action = handle_key_event(app, key, view_height);

            match action {
                ui::keybindings::Action::Refresh => {
                    app.notify("Refreshing...");
                    terminal.draw(|f| app.render(f))?;

                    match imap_client.fetch_emails(app.current_folder.imap_name(), 0, 50) {
                        Ok(emails) => {
                            app.set_emails(emails);
                            app.notify("Refreshed");
                        }
                        Err(e) => {
                            app.notify_error(&format!("Error: {}", e));
                        }
                    }
                }
                ui::keybindings::Action::ChangeFolder(folder) => {
                    app.notify(&format!("Loading {}...", folder.display_name()));
                    terminal.draw(|f| app.render(f))?;

                    match imap_client.fetch_emails(folder.imap_name(), 0, 50) {
                        Ok(emails) => {
                            app.current_folder = folder;
                            app.set_emails(emails);
                            app.clear_notification();
                        }
                        Err(e) => {
                            app.notify_error(&format!("Error: {}", e));
                        }
                    }
                }
                ui::keybindings::Action::SendEmail => {
                    if app.compose.to.is_empty() {
                        app.notify_error("'To' field is empty");
                    } else {
                        app.notify("Sending...");
                        terminal.draw(|f| app.render(f))?;

                        let access_token = config.access_token.as_ref().unwrap();
                        match email::send_email(
                            &config.email,
                            &app.compose.to,
                            &app.compose.cc,
                            &app.compose.subject,
                            &app.compose.body,
                            access_token,
                            app.compose.in_reply_to.as_deref(),
                            &app.compose.references,
                        ) {
                            Ok(_) => {
                                app.notify("Sent");
                                app.view = ui::app::View::Inbox;
                                app.compose = Default::default();
                            }
                            Err(e) => {
                                app.notify_error(&format!("Send failed: {}", e));
                            }
                        }
                    }
                }
                ui::keybindings::Action::SaveDraft => {
                    if app.compose.to.is_empty() && app.compose.cc.is_empty() {
                        app.notify_error("'To' or 'Cc' field is required");
                    } else {
                        app.notify("Saving draft...");
                        terminal.draw(|f| app.render(f))?;

                        match imap_client.save_draft(
                            &config.email,
                            &app.compose.to,
                            &app.compose.cc,
                            &app.compose.subject,
                            &app.compose.body,
                        ) {
                            Ok(_) => {
                                app.notify("Draft saved");
                                app.view = ui::app::View::Inbox;
                                app.compose = Default::default();
                            }
                            Err(e) => {
                                app.notify_error(&format!("Draft failed: {}", e));
                            }
                        }
                    }
                }
                ui::keybindings::Action::EditDraft => {
                    app.edit_draft();
                }
                ui::keybindings::Action::ArchiveEmail => {
                    let uids: Vec<u32> = if app.selected.is_empty() {
                        app.selected_email().map(|e| e.uid).into_iter().collect()
                    } else {
                        app.get_selected_uids()
                    };
                    
                    if !uids.is_empty() {
                        let count = uids.len();
                        app.notify(&format!("Archiving {}...", count));
                        terminal.draw(|f| app.render(f))?;

                        let mut success = 0;
                        for uid in &uids {
                            if imap_client.archive_email(*uid).is_ok() {
                                success += 1;
                            }
                        }
                        
                        app.emails.retain(|e| !uids.contains(&e.uid));
                        app.clear_selection();
                        if app.list_state.selected().unwrap_or(0) >= app.emails.len() && !app.emails.is_empty() {
                            app.list_state.select(Some(app.emails.len() - 1));
                        }
                        app.notify(&format!("Archived {}", success));
                    }
                }
                ui::keybindings::Action::DeleteEmail => {
                    let uids: Vec<u32> = if app.selected.is_empty() {
                        app.selected_email().map(|e| e.uid).into_iter().collect()
                    } else {
                        app.get_selected_uids()
                    };
                    
                    if !uids.is_empty() {
                        let count = uids.len();
                        app.notify(&format!("Deleting {}...", count));
                        terminal.draw(|f| app.render(f))?;

                        let mut success = 0;
                        for uid in &uids {
                            if imap_client.delete_email(*uid).is_ok() {
                                success += 1;
                            }
                        }
                        
                        app.emails.retain(|e| !uids.contains(&e.uid));
                        app.clear_selection();
                        if app.list_state.selected().unwrap_or(0) >= app.emails.len() && !app.emails.is_empty() {
                            app.list_state.select(Some(app.emails.len() - 1));
                        }
                        app.notify(&format!("Deleted {}", success));
                    }
                }
                ui::keybindings::Action::MarkAsRead(uid) => {
                    let _ = imap_client.mark_as_read(uid);
                    if let Some(idx) = app.list_state.selected() {
                        if let Some(email) = app.emails.get_mut(idx) {
                            email.seen = true;
                        }
                    }
                }
                ui::keybindings::Action::FetchThread => {
                    if let Some(email) = app.selected_email().cloned() {
                        if !email.references.is_empty() || email.in_reply_to.is_some() {
                            if let Ok(thread) = imap_client.fetch_thread(&email) {
                                app.set_reply_chain_from_thread(thread);
                            }
                        }
                    }
                }
                ui::keybindings::Action::RemindEmail(uid, duration_str) => {
                    match reminders::calculate_return_time(&duration_str) {
                        Ok(return_time) => {
                            reminders.add_reminder(uid, return_time);
                            if let Err(e) = reminders.save() {
                                app.notify_error(&format!("Failed to save reminder: {}", e));
                            } else {
                                let msg = format!("Email reminded for {}", duration_str);
                                app.notify(&msg);
                                // Move email to archive
                                let _ = imap_client.archive_email(uid);
                                app.emails.retain(|e| e.uid != uid);
                                if app.list_state.selected().unwrap_or(0) >= app.emails.len() && !app.emails.is_empty() {
                                    app.list_state.select(Some(app.emails.len() - 1));
                                }
                            }
                        }
                        Err(e) => {
                            app.notify_error(&format!("Invalid time format: {}", e));
                        }
                    }
                }
                ui::keybindings::Action::None => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
