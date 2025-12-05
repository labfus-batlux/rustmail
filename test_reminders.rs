use std::path::PathBuf;
use chrono::{DateTime, Duration, Utc};

fn parse_duration(input: &str) -> Result<Duration, String> {
    let parts: Vec<&str> = input.trim().split_whitespace().collect();
    
    if parts.len() != 2 {
        return Err("Invalid format. Use: '1 hour', '2 days', etc.".to_string());
    }

    let count: i64 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid number: {}", parts[0]))?;
    
    let unit = parts[1].to_lowercase();
    
    let duration = match unit.as_str() {
        "minute" | "minutes" => Duration::minutes(count),
        "hour" | "hours" => Duration::hours(count),
        "day" | "days" => Duration::days(count),
        "week" | "weeks" => Duration::days(count * 7),
        "month" | "months" => Duration::days(count * 30),
        _ => return Err(format!("Unknown time unit: {}", unit)),
    };

    Ok(duration)
}

fn main() {
    let test_cases = vec![
        "1 hour",
        "2 hours",
        "1 day",
        "2 days",
        "1 week",
        "3 weeks",
        "1 month",
    ];
    
    for test in test_cases {
        match parse_duration(test) {
            Ok(duration) => {
                let now = Utc::now();
                let return_time = now + duration;
                println!("{:20} -> {} (in {} minutes)", test, return_time.format("%H:%M:%S"), duration.num_minutes());
            }
            Err(e) => {
                println!("{:20} -> ERROR: {}", test, e);
            }
        }
    }
}
