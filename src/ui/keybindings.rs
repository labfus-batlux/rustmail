use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use super::app::{App, ComposeField, EditMode, Folder, View, VimOperator};

pub enum Action {
    None,
    Refresh,
    SendEmail,
    SaveDraft,
    EditDraft,
    DeleteEmail,
    ArchiveEmail,
    MarkAsRead(u32),
    ChangeFolder(Folder),
    FetchThread,
    RemindEmail(u32, String),
}

pub fn handle_key_event(app: &mut App, key: KeyEvent, view_height: u16) -> Action {
    // Clear notification on any key press
    app.clear_notification();
    
    match app.view {
        View::Inbox => handle_inbox_keys(app, key),
        View::EmailView => handle_email_view_keys(app, key, view_height),
        View::Compose => handle_compose_keys(app, key),
        View::Help => handle_help_keys(app, key),
        View::Search => handle_search_keys(app, key),
        View::Command => handle_command_keys(app, key),
        View::Remind => handle_remind_keys(app, key),
    }
}

fn handle_inbox_keys(app: &mut App, key: KeyEvent) -> Action {
    // Handle pending 'g' commands
    if let Some(pending) = app.pending_command {
        if key.code == KeyCode::Esc {
            app.pending_command = None;
            return Action::None;
        }
        app.pending_command = None;
        match (pending, key.code) {
            ('g', KeyCode::Char('g')) => {
                app.select_first();
                return Action::None;
            }
            ('g', KeyCode::Char('i')) => {
                app.clear_search_filter();
                return Action::ChangeFolder(Folder::Inbox);
            }
            ('g', KeyCode::Char('t')) => {
                app.clear_search_filter();
                return Action::ChangeFolder(Folder::Sent);
            }
            ('g', KeyCode::Char('d')) => {
                app.clear_search_filter();
                return Action::ChangeFolder(Folder::Drafts);
            }
            ('g', KeyCode::Char('e')) => {
                app.clear_search_filter();
                return Action::ChangeFolder(Folder::Trash);
            }
            ('g', KeyCode::Char('a')) => {
                app.clear_search_filter();
                return Action::ChangeFolder(Folder::Archive);
            }
            _ => {}
        }
    }

    match key.code {
        // Quit
        KeyCode::Char('q') => {
            app.should_quit = true;
            Action::None
        }
        
        // Navigation
        KeyCode::Char('J') => {
            app.select_next_with_selection();
            Action::None
        }
        KeyCode::Char('K') => {
            app.select_previous_with_selection();
            Action::None
        }
        KeyCode::Char('j') | KeyCode::Down => {
            app.select_next();
            Action::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.select_previous();
            Action::None
        }
        KeyCode::Char('g') => {
            app.pending_command = Some('g');
            Action::None
        }
        KeyCode::Char('G') => {
            app.select_last();
            Action::None
        }
        
        // Open email
        KeyCode::Enter | KeyCode::Char('l') => {
            if let Some(email) = app.selected_email() {
                let uid = email.uid;
                let unread = !email.seen;
                app.view = View::EmailView;
                app.scroll_offset = 0;
                if unread {
                    return Action::MarkAsRead(uid);
                }
            }
            Action::None
        }
        
        // Compose
        KeyCode::Char('c') => {
            app.compose = Default::default();
            app.view = View::Compose;
            Action::None
        }
        
        // Actions
        KeyCode::Char('e') => {
            if app.selected_email().is_some() {
                Action::ArchiveEmail
            } else {
                Action::None
            }
        }
        KeyCode::Char('d') => {
            if app.selected_email().is_some() {
                Action::DeleteEmail
            } else {
                Action::None
            }
        }
        KeyCode::Char('s') => {
            app.toggle_star();
            Action::None
        }
        KeyCode::Char('x') => {
            app.toggle_selection();
            Action::None
        }
        KeyCode::Esc => {
            if !app.selected.is_empty() {
                app.clear_selection();
            }
            Action::None
        }
        
        // Remind
        KeyCode::Char('h') => {
            if app.selected_email().is_some() {
                app.remind = Default::default();
                app.view = View::Remind;
            }
            Action::None
        }
        
        // Importance filter
        KeyCode::Char('I') => {
            app.cycle_importance_filter();
            Action::None
        }
        
        // Refresh
        KeyCode::Char('R') => Action::Refresh,
        
        // Search
        KeyCode::Char('/') => {
            app.search = Default::default();
            app.update_search();
            app.view = View::Search;
            Action::None
        }
        
        // Command palette
        KeyCode::Char(':') => {
            app.command = Default::default();
            app.command.update_suggestions();
            app.view = View::Command;
            Action::None
        }
        
        // Help
        KeyCode::Char('?') => {
            app.view = View::Help;
            Action::None
        }
        
        _ => Action::None,
    }
}

fn handle_email_view_keys(app: &mut App, key: KeyEvent, view_height: u16) -> Action {
    match (key.modifiers, key.code) {
        // Navigation between emails
        (_, KeyCode::Char('j')) | (_, KeyCode::Down) => {
            app.select_next();
            app.scroll_offset = 0;
            Action::None
        }
        (_, KeyCode::Char('k')) | (_, KeyCode::Up) => {
            app.select_previous();
            app.scroll_offset = 0;
            Action::None
        }
        
        // Scrolling within email
        (KeyModifiers::CONTROL, KeyCode::Char('d')) => {
            app.half_page_down(view_height);
            Action::None
        }
        (KeyModifiers::CONTROL, KeyCode::Char('u')) => {
            app.half_page_up(view_height);
            Action::None
        }
        (_, KeyCode::Char(' ')) => {
            app.half_page_down(view_height);
            Action::None
        }
        (KeyModifiers::SHIFT, KeyCode::Char(' ')) => {
            app.half_page_up(view_height);
            Action::None
        }
        
        // Reply/Forward
        (_, KeyCode::Char('r')) => {
            app.start_reply(false);
            Action::FetchThread
        }
        (_, KeyCode::Char('a')) => {
            app.start_reply(true);
            Action::FetchThread
        }
        (_, KeyCode::Char('f')) => {
            app.start_forward();
            Action::None
        }
        
        // Actions
        (_, KeyCode::Char('e')) => {
            // In drafts folder, 'e' edits the draft
            // In other folders, 'e' archives
            if app.current_folder == Folder::Drafts {
                Action::EditDraft
            } else {
                Action::ArchiveEmail
            }
        }
        (_, KeyCode::Char('d')) => Action::DeleteEmail,
        (_, KeyCode::Char('s')) => {
            app.toggle_star();
            Action::None
        }
        
        // Go back
        (_, KeyCode::Char('q')) | (_, KeyCode::Esc) | (_, KeyCode::Left) => {
            app.view = View::Inbox;
            app.scroll_offset = 0;
            Action::None
        }
        
        // Help
        (_, KeyCode::Char('?')) => {
            app.view = View::Help;
            Action::None
        }
        
        _ => Action::None,
    }
}

fn handle_search_keys(app: &mut App, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            app.search.active = false;
            app.view = View::Inbox;
            Action::None
        }
        KeyCode::Enter => {
            if let Some(&idx) = app.search.results.get(app.search.selected) {
                app.list_state.select(Some(idx));
                app.search.active = true;
                app.view = View::Inbox;
            }
            Action::None
        }
        KeyCode::Down | KeyCode::Tab | KeyCode::Char('j') => {
            if !app.search.results.is_empty() {
                app.search.selected = (app.search.selected + 1) % app.search.results.len();
            }
            Action::None
        }
        KeyCode::Up | KeyCode::BackTab | KeyCode::Char('k') if key.modifiers == KeyModifiers::CONTROL => {
            if !app.search.results.is_empty() {
                app.search.selected = app.search.selected.checked_sub(1).unwrap_or(app.search.results.len() - 1);
            }
            Action::None
        }
        KeyCode::Backspace => {
            app.search.query.pop();
            app.update_search();
            Action::None
        }
        KeyCode::Char(c) => {
            app.search.query.push(c);
            app.update_search();
            Action::None
        }
        _ => Action::None,
    }
}

fn execute_command(app: &mut App, cmd: &str) -> Action {
    match cmd {
        "q" | "quit" => {
            app.should_quit = true;
            Action::None
        }
        "refresh" | "r" => {
            Action::Refresh
        }
        "inbox" => {
            app.clear_search_filter();
            Action::ChangeFolder(Folder::Inbox)
        }
        "sent" => {
            app.clear_search_filter();
            Action::ChangeFolder(Folder::Sent)
        }
        "drafts" => {
            app.clear_search_filter();
            Action::ChangeFolder(Folder::Drafts)
        }
        "trash" => {
            app.clear_search_filter();
            Action::ChangeFolder(Folder::Trash)
        }
        "archive" => {
            app.clear_search_filter();
            Action::ChangeFolder(Folder::Archive)
        }
        _ => {
            app.notify_error(&format!("Unknown command: {}", cmd));
            Action::None
        }
    }
}

fn handle_command_keys(app: &mut App, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            app.command = Default::default();
            app.view = View::Inbox;
            Action::None
        }
        KeyCode::Enter => {
            let cmd = if let Some(selected) = app.command.get_selected_command() {
                selected.to_string()
            } else {
                app.command.input.trim().to_lowercase()
            };
            app.view = View::Inbox;
            let action = execute_command(app, &cmd);
            app.command = Default::default();
            action
        }
        KeyCode::Tab | KeyCode::Down => {
            app.command.select_next();
            Action::None
        }
        KeyCode::BackTab | KeyCode::Up => {
            app.command.select_previous();
            Action::None
        }
        KeyCode::Backspace => {
            app.command.input.pop();
            app.command.update_suggestions();
            Action::None
        }
        KeyCode::Char(c) => {
            app.command.input.push(c);
            app.command.update_suggestions();
            Action::None
        }
        _ => Action::None,
    }
}

fn handle_compose_keys(app: &mut App, key: KeyEvent) -> Action {
    if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('s') {
        return Action::SendEmail;
    }

    match app.compose.edit_mode {
        EditMode::Insert => handle_compose_insert(app, key),
        EditMode::Normal => handle_compose_normal(app, key),
    }
}

fn handle_compose_insert(app: &mut App, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            app.compose.edit_mode = EditMode::Normal;
            Action::None
        }
        KeyCode::Tab => {
            app.compose.active_field = match app.compose.active_field {
                ComposeField::To => ComposeField::Cc,
                ComposeField::Cc => ComposeField::Subject,
                ComposeField::Subject => ComposeField::Body,
                ComposeField::Body => ComposeField::To,
            };
            app.sync_cursor_to_field();
            Action::None
        }
        KeyCode::Backspace => {
            app.delete_char_before();
            Action::None
        }
        KeyCode::Enter => {
            if app.compose.active_field == ComposeField::Body {
                app.insert_char('\n');
            } else {
                app.compose.active_field = match app.compose.active_field {
                    ComposeField::To => ComposeField::Cc,
                    ComposeField::Cc => ComposeField::Subject,
                    ComposeField::Subject => ComposeField::Body,
                    ComposeField::Body => ComposeField::Body,
                };
                app.sync_cursor_to_field();
            }
            Action::None
        }
        KeyCode::Left => {
            app.move_cursor_left();
            Action::None
        }
        KeyCode::Right => {
            app.move_cursor_right();
            Action::None
        }
        KeyCode::Char(c) => {
            app.insert_char(c);
            Action::None
        }
        _ => Action::None,
    }
}

fn handle_compose_normal(app: &mut App, key: KeyEvent) -> Action {
    let pending_op = app.compose.vim.operator.clone();
    
    match key.code {
        // Number prefix for count
        KeyCode::Char(c @ '1'..='9') => {
            let digit = c.to_digit(10).unwrap() as usize;
            app.compose.vim.count = Some(app.compose.vim.count.unwrap_or(0) * 10 + digit);
            Action::None
        }
        KeyCode::Char('0') if app.compose.vim.count.is_some() => {
            app.compose.vim.count = Some(app.compose.vim.count.unwrap() * 10);
            Action::None
        }
        
        // Operators
        KeyCode::Char('d') => {
            if pending_op == VimOperator::Delete {
                app.delete_current_line();
                app.reset_vim_state();
            } else {
                app.compose.vim.operator = VimOperator::Delete;
            }
            Action::None
        }
        KeyCode::Char('c') => {
            if pending_op == VimOperator::Change {
                app.delete_current_line();
                app.compose.edit_mode = EditMode::Insert;
                app.reset_vim_state();
            } else {
                app.compose.vim.operator = VimOperator::Change;
            }
            Action::None
        }
        
        // Enter insert mode
        KeyCode::Char('i') => {
            app.reset_vim_state();
            app.compose.edit_mode = EditMode::Insert;
            Action::None
        }
        KeyCode::Char('a') => {
            app.reset_vim_state();
            app.compose.edit_mode = EditMode::Insert;
            app.move_cursor_right();
            Action::None
        }
        KeyCode::Char('A') => {
            app.reset_vim_state();
            app.compose.edit_mode = EditMode::Insert;
            app.compose.cursor_pos = app.get_current_field().chars().count();
            Action::None
        }
        KeyCode::Char('I') => {
            app.reset_vim_state();
            app.compose.edit_mode = EditMode::Insert;
            app.compose.cursor_pos = 0;
            Action::None
        }
        KeyCode::Char('o') => {
            app.reset_vim_state();
            app.compose.edit_mode = EditMode::Insert;
            let len = app.get_current_field().chars().count();
            app.compose.cursor_pos = len;
            app.insert_char('\n');
            Action::None
        }
        
        // Movement (with count support)
        KeyCode::Char('h') | KeyCode::Left => {
            let count = app.get_vim_count();
            for _ in 0..count {
                app.move_cursor_left();
            }
            app.reset_vim_state();
            Action::None
        }
        KeyCode::Char('l') | KeyCode::Right => {
            let count = app.get_vim_count();
            for _ in 0..count {
                app.move_cursor_right();
            }
            app.reset_vim_state();
            Action::None
        }
        KeyCode::Char('w') => {
            let count = app.get_vim_count();
            match pending_op {
                VimOperator::Delete => {
                    for _ in 0..count {
                        app.delete_word_forward();
                    }
                }
                VimOperator::Change => {
                    for _ in 0..count {
                        app.delete_word_forward();
                    }
                    app.compose.edit_mode = EditMode::Insert;
                }
                VimOperator::None => {
                    for _ in 0..count {
                        app.move_cursor_word_forward();
                    }
                }
            }
            app.reset_vim_state();
            Action::None
        }
        KeyCode::Char('b') => {
            let count = app.get_vim_count();
            for _ in 0..count {
                app.move_cursor_word_backward();
            }
            app.reset_vim_state();
            Action::None
        }
        KeyCode::Char('e') => {
            let count = app.get_vim_count();
            match pending_op {
                VimOperator::Delete => {
                    for _ in 0..count {
                        app.delete_to_word_end();
                    }
                }
                VimOperator::Change => {
                    for _ in 0..count {
                        app.delete_to_word_end();
                    }
                    app.compose.edit_mode = EditMode::Insert;
                }
                VimOperator::None => {
                    for _ in 0..count {
                        app.move_cursor_word_end();
                    }
                }
            }
            app.reset_vim_state();
            Action::None
        }
        KeyCode::Char('0') => {
            app.compose.cursor_pos = 0;
            app.reset_vim_state();
            Action::None
        }
        KeyCode::Char('$') => {
            app.compose.cursor_pos = app.get_current_field().chars().count().saturating_sub(1);
            app.reset_vim_state();
            Action::None
        }
        
        // Editing
        KeyCode::Char('x') => {
            let count = app.get_vim_count();
            for _ in 0..count {
                app.delete_char_at();
            }
            app.reset_vim_state();
            Action::None
        }
        
        // Field navigation
        KeyCode::Char('j') | KeyCode::Down => {
            app.compose.active_field = match app.compose.active_field {
                ComposeField::To => ComposeField::Cc,
                ComposeField::Cc => ComposeField::Subject,
                ComposeField::Subject => ComposeField::Body,
                ComposeField::Body => ComposeField::Body,
            };
            app.sync_cursor_to_field();
            app.reset_vim_state();
            Action::None
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.compose.active_field = match app.compose.active_field {
                ComposeField::To => ComposeField::To,
                ComposeField::Cc => ComposeField::To,
                ComposeField::Subject => ComposeField::Cc,
                ComposeField::Body => ComposeField::Subject,
            };
            app.sync_cursor_to_field();
            app.reset_vim_state();
            Action::None
        }
        
        // Quit compose
        KeyCode::Char('q') => {
            app.reset_vim_state();
            app.view = View::Inbox;
            Action::None
        }
        KeyCode::Esc => {
            if pending_op != VimOperator::None || app.compose.vim.count.is_some() {
                app.reset_vim_state();
            } else {
                // Check if compose has content to save as draft
                let has_content = !app.compose.to.is_empty()
                    || !app.compose.cc.is_empty()
                    || !app.compose.subject.is_empty()
                    || !app.compose.body.is_empty();
                
                if has_content {
                    return Action::SaveDraft;
                } else {
                    app.view = View::Inbox;
                }
            }
            Action::None
        }
        
        _ => {
            app.reset_vim_state();
            Action::None
        }
    }
}

fn handle_help_keys(app: &mut App, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => {
            app.view = View::Inbox;
            Action::None
        }
        _ => Action::None,
    }
}

fn handle_remind_keys(app: &mut App, key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Esc => {
            app.view = View::Inbox;
            Action::None
        }
        KeyCode::Enter => {
            if let Some(email) = app.selected_email() {
                let uid = email.uid;
                let duration_str = app.remind.input.trim().to_string();
                app.remind = Default::default();
                app.view = View::Inbox;
                Action::RemindEmail(uid, duration_str)
            } else {
                Action::None
            }
        }
        KeyCode::Backspace => {
            app.remind.input.pop();
            if app.remind.cursor > 0 {
                app.remind.cursor -= 1;
            }
            Action::None
        }
        KeyCode::Char(c) => {
            app.remind.input.push(c);
            app.remind.cursor += 1;
            Action::None
        }
        _ => Action::None,
    }
}
