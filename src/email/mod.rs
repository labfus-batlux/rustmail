use anyhow::Result;
use chrono::{DateTime, Utc};
use html2text::from_read;
use imap::{Authenticator, Session};
use mail_parser::MessageParser;
use native_tls::TlsStream;
use std::net::TcpStream;

use crate::auth::build_oauth2_string;

fn html_to_text(html: &str) -> String {
    let text = from_read(html.as_bytes(), 80);
    // Remove reference-style link definitions like [1]: https://...
    text.lines()
        .filter(|line| {
            let trimmed = line.trim();
            !(trimmed.starts_with('[') && trimmed.contains("]: http"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

struct OAuth2Authenticator(String);

impl Authenticator for OAuth2Authenticator {
    type Response = String;
    fn process(&self, _data: &[u8]) -> Self::Response {
        self.0.clone()
    }
}

#[derive(Debug, Clone)]
pub struct Email {
    pub uid: u32,
    pub subject: String,
    pub from: String,
    pub from_address: String,
    pub date: Option<DateTime<Utc>>,
    pub body: String,
    pub seen: bool,
    pub important: bool,
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
}

pub struct ImapClient {
    session: Session<TlsStream<TcpStream>>,
}

impl ImapClient {
    pub fn connect(email: &str, access_token: &str) -> Result<Self> {
        let tls = native_tls::TlsConnector::builder().build()?;
        let client = imap::connect(("imap.gmail.com", 993), "imap.gmail.com", &tls)?;

        let oauth2_token = build_oauth2_string(email, access_token);
        let session = client
            .authenticate("XOAUTH2", &OAuth2Authenticator(oauth2_token))
            .map_err(|(e, _)| e)?;

        Ok(Self { session })
    }

    pub fn list_folders(&mut self) -> Result<Vec<String>> {
        let folders = self.session.list(Some(""), Some("*"))?;
        Ok(folders.iter().map(|f| f.name().to_string()).collect())
    }

    pub fn select_folder(&mut self, folder: &str) -> Result<u32> {
        let mailbox = self.session.select(folder)?;
        Ok(mailbox.exists)
    }

    pub fn fetch_emails(&mut self, folder: &str, start: u32, count: u32) -> Result<Vec<Email>> {
        let total = self.session.select(folder)?.exists;
        if total == 0 {
            return Ok(vec![]);
        }

        let end = total;
        let start_seq = if total > start + count {
            total - start - count + 1
        } else {
            1
        };
        let range = format!("{}:{}", start_seq, end.saturating_sub(start).max(1));

        // Get UIDs of important messages using Gmail's search extension
        let important_uids: std::collections::HashSet<u32> = self
            .session
            .uid_search("X-GM-RAW \"is:important\"")
            .map(|uids| uids.into_iter().collect())
            .unwrap_or_default();

        let messages = self.session.fetch(&range, "(UID FLAGS BODY.PEEK[])")?;
        let parser = MessageParser::default();

        let mut emails: Vec<Email> = messages
            .iter()
            .filter_map(|msg| {
                let uid = msg.uid?;
                let body = msg.body()?;
                let parsed = parser.parse(body)?;

                let subject = parsed
                    .subject()
                    .unwrap_or("(No Subject)")
                    .to_string();

                let from_addr = parsed
                    .from()
                    .and_then(|f| f.first());
                
                let from = from_addr
                    .map(|a| {
                        a.name()
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| a.address().unwrap_or("").to_string())
                    })
                    .unwrap_or_else(|| "(Unknown)".to_string());
                
                let from_address = from_addr
                    .and_then(|a| a.address())
                    .unwrap_or("")
                    .to_string();

                let date = parsed.date().map(|d| {
                    DateTime::from_timestamp(d.to_timestamp(), 0).unwrap_or_default()
                });

                let body_text = if let Some(html) = parsed.body_html(0) {
                    html_to_text(&html)
                } else if let Some(text) = parsed.body_text(0) {
                    text.to_string()
                } else {
                    String::new()
                };

                let seen = msg.flags().iter().any(|f| matches!(f, imap::types::Flag::Seen));
                let important = important_uids.contains(&uid);

                let message_id = parsed.message_id().map(|s| s.to_string());
                let in_reply_to = parsed.in_reply_to()
                    .as_text_list()
                    .and_then(|v| v.first().map(|s| s.to_string()));
                let references: Vec<String> = parsed
                    .references()
                    .as_text_list()
                    .map(|v| v.iter().map(|s| s.to_string()).collect())
                    .unwrap_or_default();

                Some(Email {
                    uid,
                    subject,
                    from,
                    from_address,
                    date,
                    body: body_text,
                    seen,
                    important,
                    message_id,
                    in_reply_to,
                    references,
                })
            })
            .collect();

        emails.reverse();
        Ok(emails)
    }

    pub fn mark_as_read(&mut self, uid: u32) -> Result<()> {
        self.session
            .uid_store(uid.to_string(), "+FLAGS (\\Seen)")?;
        Ok(())
    }

    pub fn delete_email(&mut self, uid: u32) -> Result<()> {
        self.session
            .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")?;
        self.session.expunge()?;
        Ok(())
    }

    pub fn archive_email(&mut self, uid: u32) -> Result<()> {
        // Move to All Mail (Gmail's archive)
        self.session.uid_mv(uid.to_string(), "[Gmail]/All Mail")?;
        Ok(())
    }

    pub fn fetch_thread(&mut self, email: &Email) -> Result<Vec<Email>> {
        let mut message_ids: Vec<String> = email.references.clone();
        if let Some(ref in_reply_to) = email.in_reply_to {
            if !message_ids.contains(in_reply_to) {
                message_ids.push(in_reply_to.clone());
            }
        }
        if let Some(ref msg_id) = email.message_id {
            if !message_ids.contains(msg_id) {
                message_ids.push(msg_id.clone());
            }
        }

        if message_ids.is_empty() {
            return Ok(vec![email.clone()]);
        }

        fn build_or_query(ids: &[String]) -> String {
            match ids.len() {
                0 => String::new(),
                1 => format!("HEADER Message-ID \"{}\"", ids[0]),
                _ => {
                    let first = format!("HEADER Message-ID \"{}\"", ids[0]);
                    let rest = build_or_query(&ids[1..]);
                    format!("OR {} {}", first, rest)
                }
            }
        }

        let search_query = build_or_query(&message_ids);

        let uids = self.session.uid_search(&search_query)?;
        if uids.is_empty() {
            return Ok(vec![email.clone()]);
        }

        let uid_list: String = uids.iter().map(|u| u.to_string()).collect::<Vec<_>>().join(",");
        
        // Get important UIDs for this thread
        let important_uids: std::collections::HashSet<u32> = self
            .session
            .uid_search("X-GM-RAW \"is:important\"")
            .map(|uids| uids.into_iter().collect())
            .unwrap_or_default();
        
        let messages = self.session.uid_fetch(&uid_list, "(UID FLAGS BODY.PEEK[])")?;
        let parser = MessageParser::default();

        let mut emails: Vec<Email> = messages
            .iter()
            .filter_map(|msg| {
                let uid = msg.uid?;
                let body = msg.body()?;
                let parsed = parser.parse(body)?;

                let subject = parsed.subject().unwrap_or("(No Subject)").to_string();
                let from_addr = parsed.from().and_then(|f| f.first());
                let from = from_addr
                    .map(|a| a.name().map(|n| n.to_string()).unwrap_or_else(|| a.address().unwrap_or("").to_string()))
                    .unwrap_or_else(|| "(Unknown)".to_string());
                let from_address = from_addr.and_then(|a| a.address()).unwrap_or("").to_string();
                let date = parsed.date().map(|d| DateTime::from_timestamp(d.to_timestamp(), 0).unwrap_or_default());
                let body_text = if let Some(html) = parsed.body_html(0) {
                    html_to_text(&html)
                } else if let Some(text) = parsed.body_text(0) {
                    text.to_string()
                } else {
                    String::new()
                };
                let seen = msg.flags().iter().any(|f| matches!(f, imap::types::Flag::Seen));
                let important = important_uids.contains(&uid);
                let message_id = parsed.message_id().map(|s| s.to_string());
                let in_reply_to = parsed.in_reply_to()
                    .as_text_list()
                    .and_then(|v| v.first().map(|s| s.to_string()));
                let references: Vec<String> = parsed
                    .references()
                    .as_text_list()
                    .map(|v| v.iter().map(|s| s.to_string()).collect())
                    .unwrap_or_default();

                Some(Email { uid, subject, from, from_address, date, body: body_text, seen, important, message_id, in_reply_to, references })
            })
            .collect();

        emails.sort_by(|a, b| a.date.cmp(&b.date));
        Ok(emails)
    }

    pub fn save_draft(&mut self, from: &str, to: &str, cc: &str, subject: &str, body: &str) -> Result<()> {
        // Convert message to string format for IMAP
        let email_bytes = format!(
            "From: {}\r\nTo: {}\r\n{}\r\nSubject: {}\r\nContent-Type: text/plain; charset=\"UTF-8\"\r\n\r\n{}",
            from,
            to,
            if cc.is_empty() { String::new() } else { format!("Cc: {}\r\n", cc) },
            subject,
            body
        );
        
        // Append to Drafts folder
        self.session.append("[Gmail]/Drafts", email_bytes.as_bytes())?;
        Ok(())
    }

    pub fn logout(mut self) -> Result<()> {
        self.session.logout()?;
        Ok(())
    }
}

pub fn send_email(
    from: &str,
    to: &str,
    cc: &str,
    subject: &str,
    body: &str,
    access_token: &str,
    in_reply_to: Option<&str>,
    references: &[String],
) -> Result<()> {
    use lettre::{
        message::header::ContentType,
        transport::smtp::{
            authentication::{Credentials, Mechanism},
            client::{Tls, TlsParameters},
        },
        Message, SmtpTransport, Transport,
    };

    let mut builder = Message::builder()
        .from(from.parse()?)
        .to(to.parse()?)
        .subject(subject);
    
    // Add CC if provided
    if !cc.is_empty() {
        builder = builder.cc(cc.parse()?);
    }

    if let Some(reply_to) = in_reply_to {
        let msg_id: String = reply_to.parse().unwrap();
        builder = builder.in_reply_to(msg_id);
    }

    if !references.is_empty() {
        let refs_str: String = references.join(" ").parse().unwrap();
        builder = builder.references(refs_str);
    }

    let email = builder
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string())?;

    let creds = Credentials::new(from.to_string(), access_token.to_string());

    let tls_params = TlsParameters::builder("smtp.gmail.com".to_string())
        .build_native()?;

    let mailer = SmtpTransport::builder_dangerous("smtp.gmail.com")
        .port(465)
        .tls(Tls::Wrapper(tls_params))
        .credentials(creds)
        .authentication(vec![Mechanism::Xoauth2])
        .build();

    mailer.send(&email)?;
    Ok(())
}
