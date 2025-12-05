use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reminder {
    pub uid: u32,
    pub return_time: DateTime<Utc>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RemindersFile {
    pub reminders: Vec<Reminder>,
}

impl RemindersFile {
    fn path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?;
        let rustmail_dir = config_dir.join("rustmail");
        fs::create_dir_all(&rustmail_dir)?;
        Ok(rustmail_dir.join("reminders.json"))
    }

    pub fn load() -> Result<Self> {
        let path = Self::path()?;
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            Ok(serde_json::from_str(&content)?)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }

    pub fn add_reminder(&mut self, uid: u32, return_time: DateTime<Utc>) {
        self.reminders.push(Reminder { uid, return_time });
    }

    pub fn get_due_reminders(&self) -> Vec<u32> {
        let now = Utc::now();
        self.reminders
            .iter()
            .filter(|r| r.return_time <= now)
            .map(|r| r.uid)
            .collect()
    }

    pub fn remove_reminder(&mut self, uid: u32) {
        self.reminders.retain(|r| r.uid != uid);
    }
}

pub fn parse_duration(input: &str) -> Result<Duration> {
    let parts: Vec<&str> = input.trim().split_whitespace().collect();
    
    if parts.len() != 2 {
        return Err(anyhow::anyhow!("Invalid format. Use: '1 hour', '2 days', etc."));
    }

    let count: i64 = parts[0]
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid number: {}", parts[0]))?;
    
    let unit = parts[1].to_lowercase();
    
    let duration = match unit.as_str() {
        "minute" | "minutes" => Duration::minutes(count),
        "hour" | "hours" => Duration::hours(count),
        "day" | "days" => Duration::days(count),
        "week" | "weeks" => Duration::days(count * 7),
        "month" | "months" => Duration::days(count * 30),
        _ => return Err(anyhow::anyhow!("Unknown time unit: {}", unit)),
    };

    Ok(duration)
}

pub fn calculate_return_time(duration_str: &str) -> Result<DateTime<Utc>> {
    let duration = parse_duration(duration_str)?;
    let now = Utc::now();
    Ok(now + duration)
}
