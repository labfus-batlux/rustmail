mod auth;
mod config;
mod email;
mod ui;

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

    let result = run_app(&mut terminal, &mut app, &mut imap_client, &config);

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
                            &app.compose.subject,
                            &app.compose.body,
                            access_token,
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
                ui::keybindings::Action::ArchiveEmail => {
                    if let Some(email) = app.selected_email() {
                        let uid = email.uid;
                        app.notify("Archiving...");
                        terminal.draw(|f| app.render(f))?;

                        match imap_client.archive_email(uid) {
                            Ok(_) => {
                                let idx = app.list_state.selected().unwrap_or(0);
                                app.emails.remove(idx);
                                if idx >= app.emails.len() && !app.emails.is_empty() {
                                    app.list_state.select(Some(app.emails.len() - 1));
                                }
                                app.notify("Archived");
                            }
                            Err(e) => {
                                app.notify_error(&format!("Archive failed: {}", e));
                            }
                        }
                    }
                }
                ui::keybindings::Action::DeleteEmail => {
                    if let Some(email) = app.selected_email() {
                        let uid = email.uid;
                        app.notify("Deleting...");
                        terminal.draw(|f| app.render(f))?;

                        match imap_client.delete_email(uid) {
                            Ok(_) => {
                                let idx = app.list_state.selected().unwrap_or(0);
                                app.emails.remove(idx);
                                if idx >= app.emails.len() && !app.emails.is_empty() {
                                    app.list_state.select(Some(app.emails.len() - 1));
                                }
                                app.notify("Deleted");
                            }
                            Err(e) => {
                                app.notify_error(&format!("Delete failed: {}", e));
                            }
                        }
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
                ui::keybindings::Action::None => {}
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
