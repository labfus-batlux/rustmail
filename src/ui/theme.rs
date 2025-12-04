use ratatui::style::{Color, Modifier, Style};

pub struct Theme {
    pub bg: Color,
    pub fg: Color,
    pub fg_dim: Color,
    pub fg_muted: Color,
    pub accent: Color,
    pub accent_dim: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub selection: Color,
    pub border: Color,
    pub unread: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg: Color::Reset,
            fg: Color::White,
            fg_dim: Color::Rgb(180, 180, 180),
            fg_muted: Color::Rgb(100, 100, 100),
            accent: Color::Rgb(138, 180, 248),    // Soft blue
            accent_dim: Color::Rgb(80, 120, 180),
            success: Color::Rgb(129, 199, 132),   // Soft green
            warning: Color::Rgb(255, 183, 77),    // Soft orange
            error: Color::Rgb(229, 115, 115),     // Soft red
            selection: Color::Rgb(45, 45, 50),
            border: Color::Rgb(60, 60, 65),
            unread: Color::Rgb(138, 180, 248),
        }
    }
}

impl Theme {
    pub fn text(&self) -> Style {
        Style::default().fg(self.fg)
    }
    
    pub fn text_dim(&self) -> Style {
        Style::default().fg(self.fg_dim)
    }
    
    pub fn text_muted(&self) -> Style {
        Style::default().fg(self.fg_muted)
    }
    
    pub fn accent(&self) -> Style {
        Style::default().fg(self.accent)
    }
    
    pub fn selected(&self) -> Style {
        Style::default().bg(self.selection)
    }
    
    pub fn unread(&self) -> Style {
        Style::default().fg(self.fg).add_modifier(Modifier::BOLD)
    }
    
    pub fn border(&self) -> Style {
        Style::default().fg(self.border)
    }
    
    pub fn success(&self) -> Style {
        Style::default().fg(self.success)
    }
    
    pub fn error(&self) -> Style {
        Style::default().fg(self.error)
    }
}
