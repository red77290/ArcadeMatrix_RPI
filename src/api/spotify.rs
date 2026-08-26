use reqwest::blocking::Client;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct SpotifyNowPlaying {
    pub is_active: bool,
    pub is_playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub image_url: Option<String>,
    pub progress_ms: u32,
    pub duration_ms: u32,
    pub volume_percent: u8,
    pub last_updated: Option<Instant>,
}

pub struct SpotifyClient {
    client_id: String,
    client_secret: String,
    refresh_token: String,
    access_token: Option<String>,
    token_expires_at: Option<Instant>,
    http_client: Client,
}

impl SpotifyClient {
    pub fn new(client_id: &str, client_secret: &str, refresh_token: &str) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_millis(2500))
            .build()
            .unwrap_or_else(|_| Client::new());

        Self {
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
            refresh_token: refresh_token.to_string(),
            access_token: None,
            token_expires_at: None,
            http_client,
        }
    }

    pub fn update_credentials(
        &mut self,
        client_id: &str,
        client_secret: &str,
        refresh_token: &str,
    ) {
        if self.client_id != client_id
            || self.client_secret != client_secret
            || self.refresh_token != refresh_token
        {
            self.client_id = client_id.to_string();
            self.client_secret = client_secret.to_string();
            self.refresh_token = refresh_token.to_string();
            self.access_token = None;
            self.token_expires_at = None;
        }
    }

    fn ensure_access_token(&mut self) -> Result<&str, String> {
        let is_valid = self.access_token.is_some()
            && self
                .token_expires_at
                .map_or(false, |exp| exp > Instant::now() + Duration::from_secs(30));

        if is_valid {
            return Ok(self.access_token.as_deref().unwrap());
        }

        if self.client_id.is_empty() || self.refresh_token.is_empty() {
            return Err("Missing Spotify client_id or refresh_token".to_string());
        }

        let mut params = std::collections::HashMap::new();
        params.insert("grant_type", "refresh_token");
        params.insert("refresh_token", self.refresh_token.as_str());

        let req = self
            .http_client
            .post("https://accounts.spotify.com/api/token")
            .form(&params);

        let req = if !self.client_secret.is_empty() {
            req.basic_auth(&self.client_id, Some(&self.client_secret))
        } else {
            params.insert("client_id", self.client_id.as_str());
            self.http_client
                .post("https://accounts.spotify.com/api/token")
                .form(&params)
        };

        let resp = req
            .send()
            .map_err(|e| format!("Token request failed: {}", e))?;
        if !resp.status().is_success() {
            return Err(format!("Token request HTTP {}", resp.status()));
        }

        let json: serde_json::Value = resp
            .json()
            .map_err(|e| format!("Invalid token JSON: {}", e))?;
        let access_token = json
            .get("access_token")
            .and_then(|t| t.as_str())
            .ok_or_else(|| "No access_token in response".to_string())?;

        let expires_in = json
            .get("expires_in")
            .and_then(|e| e.as_u64())
            .unwrap_or(3600);

        self.access_token = Some(access_token.to_string());
        self.token_expires_at = Some(Instant::now() + Duration::from_secs(expires_in));

        Ok(self.access_token.as_deref().unwrap())
    }

    pub fn get_currently_playing(&mut self) -> Result<SpotifyNowPlaying, String> {
        let token = self.ensure_access_token()?.to_string();

        let resp = self
            .http_client
            .get("https://api.spotify.com/v1/me/player")
            .bearer_auth(&token)
            .send()
            .map_err(|e| format!("Player request failed: {}", e))?;

        if resp.status() == reqwest::StatusCode::NO_CONTENT {
            return Ok(SpotifyNowPlaying {
                is_active: false,
                is_playing: false,
                last_updated: Some(Instant::now()),
                ..Default::default()
            });
        }

        if !resp.status().is_success() {
            return Err(format!("Player request HTTP {}", resp.status()));
        }

        let json: serde_json::Value = resp
            .json()
            .map_err(|e| format!("Invalid player JSON: {}", e))?;
        let mut status = SpotifyNowPlaying {
            is_active: true,
            is_playing: json
                .get("is_playing")
                .and_then(|p| p.as_bool())
                .unwrap_or(false),
            progress_ms: json
                .get("progress_ms")
                .and_then(|p| p.as_u64())
                .unwrap_or(0) as u32,
            last_updated: Some(Instant::now()),
            ..Default::default()
        };

        if let Some(device) = json.get("device") {
            status.volume_percent = device
                .get("volume_percent")
                .and_then(|v| v.as_u64())
                .unwrap_or(50) as u8;
        }

        if let Some(item) = json.get("item") {
            status.title = item
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            status.duration_ms = item
                .get("duration_ms")
                .and_then(|d| d.as_u64())
                .unwrap_or(0) as u32;

            if let Some(artists) = item.get("artists").and_then(|a| a.as_array()) {
                let artist_names: Vec<&str> = artists
                    .iter()
                    .filter_map(|art| art.get("name").and_then(|n| n.as_str()))
                    .collect();
                status.artist = artist_names.join(", ");
            }

            if let Some(album) = item.get("album") {
                status.album = album
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();

                if let Some(images) = album.get("images").and_then(|i| i.as_array()) {
                    // Pick 64x64 or the smallest available image
                    let smallest = images
                        .iter()
                        .min_by_key(|img| img.get("width").and_then(|w| w.as_u64()).unwrap_or(640));
                    if let Some(img) = smallest.or_else(|| images.first()) {
                        status.image_url = img
                            .get("url")
                            .and_then(|u| u.as_str())
                            .map(|s| s.to_string());
                    }
                }
            }
        }

        Ok(status)
    }
}
