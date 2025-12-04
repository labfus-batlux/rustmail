use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine};
use oauth2::{
    basic::BasicClient, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    PkceCodeChallenge, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

use crate::config::Config;

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

pub struct GoogleAuth {
    client: BasicClient,
}

impl GoogleAuth {
    pub fn new(config: &Config) -> Result<Self> {
        let client = BasicClient::new(
            ClientId::new(config.client_id.clone()),
            Some(ClientSecret::new(config.client_secret.clone())),
            AuthUrl::new(GOOGLE_AUTH_URL.to_string())?,
            Some(TokenUrl::new(GOOGLE_TOKEN_URL.to_string())?),
        )
        .set_redirect_uri(RedirectUrl::new("http://localhost:8080".to_string())?);

        Ok(Self { client })
    }

    pub fn authenticate(&self) -> Result<(String, String)> {
        let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

        let (auth_url, _csrf_token) = self
            .client
            .authorize_url(CsrfToken::new_random)
            .add_scope(Scope::new(
                "https://mail.google.com/".to_string(),
            ))
            .set_pkce_challenge(pkce_challenge)
            .url();

        println!("Opening browser for authentication...");
        println!("If it doesn't open, visit: {}", auth_url);
        let _ = webbrowser::open(auth_url.as_str());

        let listener = TcpListener::bind("127.0.0.1:8080")?;
        println!("Waiting for OAuth callback on http://localhost:8080 ...");

        let (mut stream, _) = listener.accept()?;
        let mut reader = BufReader::new(&stream);
        let mut request_line = String::new();
        reader.read_line(&mut request_line)?;

        let redirect_url = request_line
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("Invalid redirect"))?;

        let code = redirect_url
            .split("code=")
            .nth(1)
            .and_then(|s| s.split('&').next())
            .ok_or_else(|| anyhow::anyhow!("No code in redirect"))?;

        let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
            <html><body><h1>Authentication successful!</h1>\
            <p>You can close this window.</p></body></html>";
        stream.write_all(response.as_bytes())?;

        let token_result = self
            .client
            .exchange_code(AuthorizationCode::new(code.to_string()))
            .set_pkce_verifier(pkce_verifier)
            .request(oauth2::reqwest::http_client)?;

        let access_token = token_result.access_token().secret().clone();
        let refresh_token = token_result
            .refresh_token()
            .map(|t| t.secret().clone())
            .unwrap_or_default();

        Ok((access_token, refresh_token))
    }

    pub fn refresh_token(&self, refresh_token: &str) -> Result<String> {
        let token_result = self
            .client
            .exchange_refresh_token(&RefreshToken::new(refresh_token.to_string()))
            .request(oauth2::reqwest::http_client)?;

        Ok(token_result.access_token().secret().clone())
    }
}

pub fn build_oauth2_string(user: &str, access_token: &str) -> String {
    format!("user={}\x01auth=Bearer {}\x01\x01", user, access_token)
}
