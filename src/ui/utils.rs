use chrono::{DateTime, Local, Utc};

pub fn relative_time(date: Option<DateTime<Utc>>) -> String {
    let Some(date) = date else {
        return String::new();
    };
    
    let now = Utc::now();
    let duration = now.signed_duration_since(date);
    
    if duration.num_minutes() < 1 {
        "now".to_string()
    } else if duration.num_minutes() < 60 {
        format!("{}m", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{}h", duration.num_hours())
    } else if duration.num_days() == 1 {
        "Yesterday".to_string()
    } else if duration.num_days() < 7 {
        date.with_timezone(&Local).format("%a").to_string()
    } else if duration.num_days() < 365 {
        date.with_timezone(&Local).format("%b %d").to_string()
    } else {
        date.with_timezone(&Local).format("%b %d, %Y").to_string()
    }
}

pub fn truncate(s: &str, max_len: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_len {
        s.to_string()
    } else if max_len <= 1 {
        "…".to_string()
    } else {
        let truncated: String = chars[..max_len - 1].iter().collect();
        format!("{}…", truncated)
    }
}

pub fn format_email_preview(body: &str, max_len: usize) -> String {
    let preview: String = body
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|c| !c.is_control())
        .collect();
    
    truncate(&preview, max_len)
}
