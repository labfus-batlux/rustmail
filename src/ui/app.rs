use crate::email::Email;
use super::theme::Theme;
use super::utils::{relative_time, truncate};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect, Margin},
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap, Padding},
    Frame,
};

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Inbox,
    EmailView,
    Compose,
    Help,
    Search,
    Command,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Folder {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Archive,
}

impl Folder {
    pub fn imap_name(&self) -> &'static str {
        match self {
            Folder::Inbox => "INBOX",
            Folder::Sent => "[Gmail]/Sent Mail",
            Folder::Drafts => "[Gmail]/Drafts",
            Folder::Trash => "[Gmail]/Trash",
            Folder::Archive => "[Gmail]/All Mail",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Folder::Inbox => "Inbox",
            Folder::Sent => "Sent",
            Folder::Drafts => "Drafts",
            Folder::Trash => "Trash",
            Folder::Archive => "Archive",
        }
    }
    
    pub fn icon(&self) -> &'static str {
        match self {
            Folder::Inbox => "󰇰",
            Folder::Sent => "󰑊",
            Folder::Drafts => "󰻣",
            Folder::Trash => "󰆴",
            Folder::Archive => "󰀼",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComposeField {
    To,
    Subject,
    Body,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComposeMode {
    New,
    Reply,
    ReplyAll,
    Forward,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditMode {
    Normal,
    Insert,
}

#[derive(Debug, Clone)]
pub struct EmailInChain {
    pub from: String,
    pub date: Option<chrono::DateTime<chrono::Utc>>,
    pub body: String,
}

#[derive(Debug)]
pub struct ComposeState {
    pub to: String,
    pub subject: String,
    pub body: String,
    pub active_field: ComposeField,
    pub mode: ComposeMode,
    pub edit_mode: EditMode,
    pub cursor_pos: usize,
    pub reply_chain: Vec<EmailInChain>,
    pub chain_scroll: u16,
}

impl Default for ComposeState {
    fn default() -> Self {
        Self {
            to: String::new(),
            subject: String::new(),
            body: String::new(),
            active_field: ComposeField::To,
            mode: ComposeMode::New,
            edit_mode: EditMode::Insert,
            cursor_pos: 0,
            reply_chain: Vec::new(),
            chain_scroll: 0,
        }
    }
}

#[derive(Debug, Default)]
pub struct SearchState {
    pub query: String,
    pub results: Vec<usize>,
    pub selected: usize,
    pub active: bool,
}

#[derive(Debug, Default)]
pub struct CommandState {
    pub input: String,
    pub cursor: usize,
}

#[derive(Debug, Clone)]
pub struct Notification {
    pub message: String,
    pub is_error: bool,
}

// ============================================================================
// App State
// ============================================================================

pub struct App {
    pub emails: Vec<Email>,
    pub list_state: ListState,
    pub view: View,
    pub scroll_offset: u16,
    pub compose: ComposeState,
    pub notification: Option<Notification>,
    pub should_quit: bool,
    pub pending_command: Option<char>,
    pub current_folder: Folder,
    pub search: SearchState,
    pub command: CommandState,
    pub theme: Theme,
    pub starred: std::collections::HashSet<u32>,
}

impl App {
    pub fn new() -> Self {
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        Self {
            emails: Vec::new(),
            list_state,
            view: View::Inbox,
            scroll_offset: 0,
            compose: ComposeState::default(),
            notification: None,
            should_quit: false,
            pending_command: None,
            current_folder: Folder::Inbox,
            search: SearchState::default(),
            command: CommandState::default(),
            theme: Theme::default(),
            starred: std::collections::HashSet::new(),
        }
    }

    pub fn notify(&mut self, message: &str) {
        self.notification = Some(Notification {
            message: message.to_string(),
            is_error: false,
        });
    }
    
    pub fn notify_error(&mut self, message: &str) {
        self.notification = Some(Notification {
            message: message.to_string(),
            is_error: true,
        });
    }

    pub fn clear_notification(&mut self) {
        self.notification = None;
    }

    pub fn toggle_star(&mut self) {
        if let Some(email) = self.selected_email() {
            let uid = email.uid;
            if self.starred.contains(&uid) {
                self.starred.remove(&uid);
            } else {
                self.starred.insert(uid);
            }
        }
    }

    pub fn start_reply(&mut self, reply_all: bool) {
        if let Some(email) = self.selected_email().cloned() {
            let mut compose = ComposeState::default();
            compose.to = email.from_address.clone();
            compose.subject = if email.subject.starts_with("Re:") {
                email.subject.clone()
            } else {
                format!("Re: {}", email.subject)
            };
            compose.mode = if reply_all { ComposeMode::ReplyAll } else { ComposeMode::Reply };
            compose.reply_chain = vec![EmailInChain {
                from: email.from.clone(),
                date: email.date,
                body: email.body.clone(),
            }];
            compose.active_field = ComposeField::Body;
            compose.edit_mode = EditMode::Insert;
            compose.cursor_pos = 0;
            self.compose = compose;
            self.view = View::Compose;
        }
    }

    pub fn start_forward(&mut self) {
        if let Some(email) = self.selected_email().cloned() {
            let mut compose = ComposeState::default();
            compose.subject = format!("Fwd: {}", email.subject);
            compose.body = format!(
                "\n\n---------- Forwarded message ----------\nFrom: {}\nSubject: {}\n\n{}",
                email.from, email.subject, email.body
            );
            compose.mode = ComposeMode::Forward;
            compose.active_field = ComposeField::To;
            compose.edit_mode = EditMode::Insert;
            self.compose = compose;
            self.view = View::Compose;
        }
    }

    // Cursor and editing methods
    pub fn get_current_field(&self) -> &str {
        match self.compose.active_field {
            ComposeField::To => &self.compose.to,
            ComposeField::Subject => &self.compose.subject,
            ComposeField::Body => &self.compose.body,
        }
    }

    pub fn get_current_field_mut(&mut self) -> &mut String {
        match self.compose.active_field {
            ComposeField::To => &mut self.compose.to,
            ComposeField::Subject => &mut self.compose.subject,
            ComposeField::Body => &mut self.compose.body,
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.compose.cursor_pos > 0 {
            self.compose.cursor_pos -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        let len = self.get_current_field().len();
        if self.compose.cursor_pos < len {
            self.compose.cursor_pos += 1;
        }
    }

    pub fn move_cursor_word_forward(&mut self) {
        let field = self.get_current_field();
        let chars: Vec<char> = field.chars().collect();
        let mut pos = self.compose.cursor_pos;
        
        while pos < chars.len() && !chars[pos].is_whitespace() {
            pos += 1;
        }
        while pos < chars.len() && chars[pos].is_whitespace() {
            pos += 1;
        }
        self.compose.cursor_pos = pos.min(field.len());
    }

    pub fn move_cursor_word_backward(&mut self) {
        let field = self.get_current_field();
        let chars: Vec<char> = field.chars().collect();
        let mut pos = self.compose.cursor_pos;
        
        if pos > 0 { pos -= 1; }
        while pos > 0 && chars[pos].is_whitespace() {
            pos -= 1;
        }
        while pos > 0 && !chars[pos - 1].is_whitespace() {
            pos -= 1;
        }
        self.compose.cursor_pos = pos;
    }

    pub fn insert_char(&mut self, c: char) {
        let pos = self.compose.cursor_pos;
        let field = self.get_current_field_mut();
        let char_pos = field.chars().take(pos).count();
        let byte_pos: usize = field.chars().take(char_pos).map(|c| c.len_utf8()).sum();
        if byte_pos <= field.len() {
            field.insert(byte_pos, c);
            self.compose.cursor_pos += 1;
        }
    }

    pub fn delete_char_before(&mut self) {
        if self.compose.cursor_pos > 0 {
            let pos = self.compose.cursor_pos - 1;
            let field = self.get_current_field_mut();
            let chars: Vec<char> = field.chars().collect();
            if pos < chars.len() {
                let byte_pos: usize = chars.iter().take(pos).map(|c| c.len_utf8()).sum();
                let char_len = chars[pos].len_utf8();
                field.replace_range(byte_pos..byte_pos + char_len, "");
                self.compose.cursor_pos -= 1;
            }
        }
    }

    pub fn delete_char_at(&mut self) {
        let pos = self.compose.cursor_pos;
        let field = self.get_current_field_mut();
        let chars: Vec<char> = field.chars().collect();
        if pos < chars.len() {
            let byte_pos: usize = chars.iter().take(pos).map(|c| c.len_utf8()).sum();
            let char_len = chars[pos].len_utf8();
            field.replace_range(byte_pos..byte_pos + char_len, "");
        }
    }

    pub fn sync_cursor_to_field(&mut self) {
        let len = self.get_current_field().chars().count();
        self.compose.cursor_pos = self.compose.cursor_pos.min(len);
    }

    // Search
    pub fn update_search(&mut self) {
        let matcher = SkimMatcherV2::default();
        let query = &self.search.query;

        if query.is_empty() {
            self.search.results = (0..self.emails.len()).collect();
        } else {
            self.search.results = self
                .emails
                .iter()
                .enumerate()
                .filter_map(|(i, email)| {
                    let text = format!("{} {} {}", email.from, email.subject, email.body);
                    matcher.fuzzy_match(&text, query).map(|_| i)
                })
                .collect();
        }
        self.search.selected = 0;
    }

    // Email list management
    pub fn set_emails(&mut self, emails: Vec<Email>) {
        self.emails = emails;
        if !self.emails.is_empty() {
            self.list_state.select(Some(0));
        }
    }

    pub fn selected_email(&self) -> Option<&Email> {
        self.list_state.selected().and_then(|i| self.emails.get(i))
    }

    fn get_visible_indices(&self) -> Vec<usize> {
        if self.search.active {
            self.search.results.clone()
        } else {
            (0..self.emails.len()).collect()
        }
    }

    pub fn select_next(&mut self) {
        let indices = self.get_visible_indices();
        if indices.is_empty() { return; }
        let current = self.list_state.selected().unwrap_or(0);
        let current_pos = indices.iter().position(|&i| i == current).unwrap_or(0);
        let next_pos = (current_pos + 1).min(indices.len() - 1);
        self.list_state.select(Some(indices[next_pos]));
    }

    pub fn select_previous(&mut self) {
        let indices = self.get_visible_indices();
        if indices.is_empty() { return; }
        let current = self.list_state.selected().unwrap_or(0);
        let current_pos = indices.iter().position(|&i| i == current).unwrap_or(0);
        self.list_state.select(Some(indices[current_pos.saturating_sub(1)]));
    }

    pub fn select_first(&mut self) {
        let indices = self.get_visible_indices();
        if !indices.is_empty() {
            self.list_state.select(Some(indices[0]));
        }
    }

    pub fn select_last(&mut self) {
        let indices = self.get_visible_indices();
        if !indices.is_empty() {
            self.list_state.select(Some(indices[indices.len() - 1]));
        }
    }

    pub fn clear_search_filter(&mut self) {
        self.search.active = false;
        self.search.query.clear();
        self.search.results.clear();
    }

    // Scrolling
    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn half_page_down(&mut self, height: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(height / 2);
    }

    pub fn half_page_up(&mut self, height: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(height / 2);
    }

    // ========================================================================
    // Rendering
    // ========================================================================

    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();
        
        // Main layout with padding
        let main_area = area.inner(Margin::new(1, 0));
        
        match self.view {
            View::Inbox => self.render_inbox(frame, main_area),
            View::EmailView => self.render_email_view(frame, main_area),
            View::Compose => self.render_compose(frame, main_area),
            View::Search => self.render_search(frame, main_area),
            View::Command => {
                self.render_inbox(frame, main_area);
                self.render_command_palette(frame);
            }
            View::Help => {
                self.render_inbox(frame, main_area);
                self.render_help(frame);
            }
        }

        self.render_status_bar(frame, area);
    }

    fn render_inbox(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(area);

        let visible_indices = self.get_visible_indices();
        let width = chunks[0].width as usize;
        
        let items: Vec<ListItem> = visible_indices
            .iter()
            .filter_map(|&i| self.emails.get(i).map(|e| (i, e)))
            .map(|(idx, email)| {
                let is_starred = self.starred.contains(&email.uid);
                let star = if is_starred { "★" } else { " " };
                let unread_marker = if email.seen { "  " } else { "● " };
                let time = relative_time(email.date);
                
                // Calculate available space for subject
                let from_width = 22;
                let time_width = time.chars().count() + 2;
                let fixed_width = 6 + from_width + time_width;
                let subject_width = width.saturating_sub(fixed_width);
                
                let style = if email.seen {
                    self.theme.text_dim()
                } else {
                    self.theme.unread()
                };

                ListItem::new(Line::from(vec![
                    Span::styled(unread_marker, self.theme.accent()),
                    Span::styled(star, Style::default().fg(self.theme.warning)),
                    Span::styled(" ", Style::default()),
                    Span::styled(format!("{:<width$}", truncate(&email.from, from_width), width = from_width), style),
                    Span::styled(truncate(&email.subject, subject_width), style),
                    Span::styled(format!("  {}", time), self.theme.text_muted()),
                ]))
            })
            .collect();

        // Map selection
        let selected_actual = self.list_state.selected();
        let visible_selected = selected_actual.and_then(|sel| {
            visible_indices.iter().position(|&i| i == sel)
        });
        let mut visible_list_state = ListState::default();
        visible_list_state.select(visible_selected);

        let title = if self.search.active {
            format!(" {} · \"{}\" ", self.current_folder.display_name(), self.search.query)
        } else {
            format!(" {} ", self.current_folder.display_name())
        };

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.border())
                    .title(title)
                    .title_style(self.theme.accent())
                    .padding(Padding::horizontal(1))
            )
            .highlight_style(self.theme.selected())
            .highlight_symbol("  ");

        frame.render_stateful_widget(list, chunks[0], &mut visible_list_state);
    }

    fn render_email_view(&mut self, frame: &mut Frame, area: Rect) {
        let Some(email) = self.selected_email().cloned() else { return };
        
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(6), Constraint::Min(0)])
            .split(area);

        // Header
        let time_full = email.date
            .map(|d| d.format("%a, %b %d, %Y at %H:%M").to_string())
            .unwrap_or_default();
        
        let header_lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(&email.from, self.theme.text().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled(&email.subject, self.theme.text_dim()),
            ]),
            Line::from(vec![
                Span::styled(time_full, self.theme.text_muted()),
            ]),
            Line::from(""),
        ];
        
        let header = Paragraph::new(header_lines)
            .block(Block::default().padding(Padding::horizontal(2)));
        frame.render_widget(header, chunks[0]);

        // Body - clean, no boxes
        let body_lines: Vec<Line> = email.body
            .lines()
            .map(|line| Line::from(Span::styled(line.to_string(), self.theme.text())))
            .collect();

        let body = Paragraph::new(body_lines)
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(self.theme.border())
                    .padding(Padding::new(2, 2, 1, 1))
            )
            .wrap(Wrap { trim: false })
            .scroll((self.scroll_offset, 0));

        frame.render_widget(body, chunks[1]);
    }

    fn render_compose(&mut self, frame: &mut Frame, area: Rect) {
        let has_chain = !self.compose.reply_chain.is_empty();
        
        let constraints = if has_chain {
            vec![
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Min(5),
            ]
        } else {
            vec![
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(10),
            ]
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(area);

        let cursor_style = if self.compose.edit_mode == EditMode::Insert {
            Style::default().bg(self.theme.accent).fg(self.theme.bg)
        } else {
            Style::default().bg(self.theme.fg_muted).fg(self.theme.bg)
        };

        let render_field = |text: &str, cursor_pos: usize, is_active: bool| -> Vec<Span> {
            if !is_active {
                return vec![Span::styled(text.to_string(), Style::default())];
            }
            
            let chars: Vec<char> = text.chars().collect();
            let pos = cursor_pos.min(chars.len());
            let before: String = chars[..pos].iter().collect();
            let cursor_char = chars.get(pos).copied().unwrap_or(' ');
            let after: String = chars.get(pos + 1..).map(|c| c.iter().collect()).unwrap_or_default();
            
            vec![
                Span::raw(before),
                Span::styled(cursor_char.to_string(), cursor_style),
                Span::raw(after),
            ]
        };

        // To field
        let to_active = self.compose.active_field == ComposeField::To;
        let to_style = if to_active { self.theme.accent() } else { self.theme.border() };
        let to_content = render_field(&self.compose.to, self.compose.cursor_pos, to_active);
        let to_input = Paragraph::new(Line::from(to_content))
            .block(Block::default().borders(Borders::ALL).border_style(to_style).title(" To "));
        frame.render_widget(to_input, chunks[0]);

        // Subject field
        let subj_active = self.compose.active_field == ComposeField::Subject;
        let subj_style = if subj_active { self.theme.accent() } else { self.theme.border() };
        let subj_content = render_field(&self.compose.subject, self.compose.cursor_pos, subj_active);
        let subj_input = Paragraph::new(Line::from(subj_content))
            .block(Block::default().borders(Borders::ALL).border_style(subj_style).title(" Subject "));
        frame.render_widget(subj_input, chunks[1]);

        // Body field
        let body_active = self.compose.active_field == ComposeField::Body;
        let body_style = if body_active { self.theme.accent() } else { self.theme.border() };
        
        // Build body text with proper multiline support
        let body_text: Text = if body_active {
            let body_str = &self.compose.body;
            let cursor_pos = self.compose.cursor_pos;
            let chars: Vec<char> = body_str.chars().collect();
            let pos = cursor_pos.min(chars.len());
            
            let mut result_lines: Vec<Line> = Vec::new();
            let mut char_idx = 0;
            let mut cursor_placed = false;
            
            for line_str in body_str.split('\n') {
                let line_chars: Vec<char> = line_str.chars().collect();
                let line_start = char_idx;
                let line_end = char_idx + line_chars.len();
                
                if !cursor_placed && pos >= line_start && pos <= line_end {
                    cursor_placed = true;
                    let pos_in_line = pos - line_start;
                    let before: String = line_chars[..pos_in_line].iter().collect();
                    let cursor_char = line_chars.get(pos_in_line).copied().unwrap_or(' ');
                    let after: String = line_chars.get(pos_in_line + 1..).map(|c| c.iter().collect()).unwrap_or_default();
                    
                    result_lines.push(Line::from(vec![
                        Span::raw(before),
                        Span::styled(cursor_char.to_string(), cursor_style),
                        Span::raw(after),
                    ]));
                } else {
                    result_lines.push(Line::from(line_str.to_string()));
                }
                
                char_idx = line_end + 1; // +1 for the newline
            }
            
            if result_lines.is_empty() {
                result_lines.push(Line::from(vec![Span::styled(" ", cursor_style)]));
            }
            
            Text::from(result_lines)
        } else {
            Text::from(self.compose.body.as_str())
        };
        
        let body_input = Paragraph::new(body_text)
            .block(Block::default().borders(Borders::ALL).border_style(body_style).title(" Message "));
        frame.render_widget(body_input, chunks[2]);

        // Reply chain
        if has_chain {
            let mut chain_lines: Vec<Line> = Vec::new();
            for email in &self.compose.reply_chain {
                let time = relative_time(email.date);
                chain_lines.push(Line::from(vec![
                    Span::styled(&email.from, self.theme.text_dim()),
                    Span::styled(format!("  {}", time), self.theme.text_muted()),
                ]));
                chain_lines.push(Line::from(""));
                for line in email.body.lines().take(10) {
                    chain_lines.push(Line::from(Span::styled(line.to_string(), self.theme.text_muted())));
                }
            }

            let chain = Paragraph::new(chain_lines)
                .block(Block::default().borders(Borders::ALL).border_style(self.theme.border()).title(" Thread "))
                .wrap(Wrap { trim: false })
                .scroll((self.compose.chain_scroll, 0));
            frame.render_widget(chain, chunks[3]);
        }
    }

    fn render_search(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        let search_input = Paragraph::new(self.search.query.as_str())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.accent())
                    .title(" Search ")
                    .padding(Padding::horizontal(1))
            );
        frame.render_widget(search_input, chunks[0]);

        let width = chunks[1].width as usize;
        let items: Vec<ListItem> = self.search.results
            .iter()
            .filter_map(|&i| self.emails.get(i))
            .map(|email| {
                let time = relative_time(email.date);
                let from_width = 22;
                let time_width = time.chars().count() + 2;
                let subject_width = width.saturating_sub(from_width + time_width + 6);
                
                let style = if email.seen { self.theme.text_dim() } else { self.theme.unread() };

                ListItem::new(Line::from(vec![
                    Span::styled(format!("{:<width$}", truncate(&email.from, from_width), width = from_width), style),
                    Span::styled(truncate(&email.subject, subject_width), style),
                    Span::styled(format!("  {}", time), self.theme.text_muted()),
                ]))
            })
            .collect();

        let mut list_state = ListState::default();
        if !self.search.results.is_empty() {
            list_state.select(Some(self.search.selected));
        }

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).border_style(self.theme.border()))
            .highlight_style(self.theme.selected())
            .highlight_symbol("  ");

        frame.render_stateful_widget(list, chunks[1], &mut list_state);
    }

    fn render_command_palette(&self, frame: &mut Frame) {
        let area = frame.area();
        let width = (area.width as f32 * 0.6) as u16;
        let popup = Rect::new(
            (area.width - width) / 2,
            area.height / 4,
            width,
            3,
        );

        frame.render_widget(Clear, popup);
        
        let input = Paragraph::new(format!(":{}", self.command.input))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.accent())
            );
        frame.render_widget(input, popup);
    }

    fn render_help(&self, frame: &mut Frame) {
        let area = frame.area();
        let popup = centered_rect(50, 60, area);

        frame.render_widget(Clear, popup);

        let help_text = vec![
            Line::from(Span::styled("Keyboard Shortcuts", self.theme.accent().add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(vec![Span::styled("j/k       ", self.theme.accent()), Span::raw("Navigate")]),
            Line::from(vec![Span::styled("Enter/l   ", self.theme.accent()), Span::raw("Open email")]),
            Line::from(vec![Span::styled("q/Esc     ", self.theme.accent()), Span::raw("Go back / Quit")]),
            Line::from(""),
            Line::from(vec![Span::styled("c         ", self.theme.accent()), Span::raw("Compose")]),
            Line::from(vec![Span::styled("r         ", self.theme.accent()), Span::raw("Reply")]),
            Line::from(vec![Span::styled("a         ", self.theme.accent()), Span::raw("Reply all")]),
            Line::from(vec![Span::styled("f         ", self.theme.accent()), Span::raw("Forward")]),
            Line::from(""),
            Line::from(vec![Span::styled("e         ", self.theme.accent()), Span::raw("Archive")]),
            Line::from(vec![Span::styled("d         ", self.theme.accent()), Span::raw("Delete")]),
            Line::from(vec![Span::styled("s         ", self.theme.accent()), Span::raw("Star/unstar")]),
            Line::from(""),
            Line::from(vec![Span::styled("/         ", self.theme.accent()), Span::raw("Search")]),
            Line::from(vec![Span::styled(":         ", self.theme.accent()), Span::raw("Command palette")]),
            Line::from(vec![Span::styled("R         ", self.theme.accent()), Span::raw("Refresh")]),
            Line::from(""),
            Line::from(vec![Span::styled("gi        ", self.theme.accent()), Span::raw("Go to Inbox")]),
            Line::from(vec![Span::styled("gt        ", self.theme.accent()), Span::raw("Go to Sent")]),
            Line::from(vec![Span::styled("gd        ", self.theme.accent()), Span::raw("Go to Drafts")]),
            Line::from(vec![Span::styled("ge        ", self.theme.accent()), Span::raw("Go to Trash")]),
            Line::from(vec![Span::styled("ga        ", self.theme.accent()), Span::raw("Go to Archive")]),
            Line::from(""),
            Line::from(vec![Span::styled("Ctrl+s    ", self.theme.accent()), Span::raw("Send (in compose)")]),
        ];

        let help = Paragraph::new(help_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(self.theme.border())
                    .title(" Help ")
                    .title_style(self.theme.accent())
                    .padding(Padding::new(2, 2, 1, 1))
            )
            .style(Style::default().bg(self.theme.bg));

        frame.render_widget(help, popup);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let status_area = Rect::new(0, area.height - 1, area.width, 1);

        let (left, right) = if let Some(notif) = &self.notification {
            let style = if notif.is_error { self.theme.error() } else { self.theme.success() };
            (Span::styled(&notif.message, style), Span::raw(""))
        } else {
            let mode = match self.view {
                View::Compose => {
                    match self.compose.edit_mode {
                        EditMode::Insert => "INSERT",
                        EditMode::Normal => "NORMAL",
                    }
                }
                _ => "",
            };
            
            let left = format!(" {} · {} emails", self.current_folder.display_name(), self.emails.len());
            let right = if mode.is_empty() {
                " ? help ".to_string()
            } else {
                format!(" {} ", mode)
            };
            
            (Span::styled(left, self.theme.text_muted()), Span::styled(right, self.theme.text_muted()))
        };

        let status = Paragraph::new(Line::from(vec![left]))
            .style(Style::default().bg(self.theme.selection));
        frame.render_widget(status, status_area);
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
