use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Google Cast message namespaces
pub const NS_CONNECTION: &str = "urn:x-cast:com.google.cast.tp.connection";
pub const NS_HEARTBEAT: &str = "urn:x-cast:com.google.cast.tp.heartbeat";
pub const NS_RECEIVER: &str = "urn:x-cast:com.google.cast.receiver";
pub const NS_MEDIA: &str = "urn:x-cast:com.google.cast.media";

/// Live status extracted from Google Cast media sessions
#[derive(Debug, Clone, Default)]
pub struct CastMediaStatus {
    pub is_active: bool,
    pub is_playing: bool,
    pub app_name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub image_url: Option<String>,
    pub current_time_sec: f32,
    pub duration_sec: f32,
    pub volume_level: f32, // 0.0 to 1.0
    pub is_muted: bool,
    pub last_updated: Option<Instant>,
}

/// Simple Protobuf wire encoder/decoder for CastMessage
/// Schema:
/// 1: protocol_version (enum 0)
/// 2: source_id (string)
/// 3: destination_id (string)
/// 4: namespace (string)
/// 5: payload_type (enum 0 = STRING)
/// 6: payload_utf8 (string)
#[derive(Debug, Clone)]
pub struct CastMessage {
    pub protocol_version: i32,
    pub source_id: String,
    pub destination_id: String,
    pub namespace: String,
    pub payload_utf8: String,
}

impl CastMessage {
    pub fn new(source_id: &str, destination_id: &str, namespace: &str, payload_utf8: &str) -> Self {
        Self {
            protocol_version: 0,
            source_id: source_id.to_string(),
            destination_id: destination_id.to_string(),
            namespace: namespace.to_string(),
            payload_utf8: payload_utf8.to_string(),
        }
    }

    /// Encode CastMessage into protobuf wire format bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // 1: protocol_version (varint, tag = (1 << 3) | 0 = 8)
        buf.push(8);
        encode_varint(self.protocol_version as u64, &mut buf);

        // 2: source_id (length-delimited, tag = (2 << 3) | 2 = 18)
        buf.push(18);
        encode_bytes(self.source_id.as_bytes(), &mut buf);

        // 3: destination_id (tag = (3 << 3) | 2 = 26)
        buf.push(26);
        encode_bytes(self.destination_id.as_bytes(), &mut buf);

        // 4: namespace (tag = (4 << 3) | 2 = 34)
        buf.push(34);
        encode_bytes(self.namespace.as_bytes(), &mut buf);

        // 5: payload_type (enum = 0, tag = (5 << 3) | 0 = 40)
        buf.push(40);
        encode_varint(0, &mut buf);

        // 6: payload_utf8 (tag = (6 << 3) | 2 = 50)
        buf.push(50);
        encode_bytes(self.payload_utf8.as_bytes(), &mut buf);

        buf
    }

    /// Decode CastMessage from raw protobuf wire format bytes
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut msg = CastMessage {
            protocol_version: 0,
            source_id: String::new(),
            destination_id: String::new(),
            namespace: String::new(),
            payload_utf8: String::new(),
        };

        let mut idx = 0;
        while idx < bytes.len() {
            let (tag_wire, n) = decode_varint(&bytes[idx..])?;
            idx += n;
            let field_num = tag_wire >> 3;
            let wire_type = tag_wire & 7;

            match (field_num, wire_type) {
                (1, 0) => {
                    let (val, n) = decode_varint(&bytes[idx..])?;
                    idx += n;
                    msg.protocol_version = val as i32;
                }
                (2, 2) => {
                    let (data, n) = decode_bytes(&bytes[idx..])?;
                    idx += n;
                    msg.source_id = String::from_utf8_lossy(data).to_string();
                }
                (3, 2) => {
                    let (data, n) = decode_bytes(&bytes[idx..])?;
                    idx += n;
                    msg.destination_id = String::from_utf8_lossy(data).to_string();
                }
                (4, 2) => {
                    let (data, n) = decode_bytes(&bytes[idx..])?;
                    idx += n;
                    msg.namespace = String::from_utf8_lossy(data).to_string();
                }
                (5, 0) => {
                    let (_val, n) = decode_varint(&bytes[idx..])?;
                    idx += n;
                }
                (6, 2) => {
                    let (data, n) = decode_bytes(&bytes[idx..])?;
                    idx += n;
                    msg.payload_utf8 = String::from_utf8_lossy(data).to_string();
                }
                (_, 0) => {
                    let (_, n) = decode_varint(&bytes[idx..])?;
                    idx += n;
                }
                (_, 2) => {
                    let (_, n) = decode_bytes(&bytes[idx..])?;
                    idx += n;
                }
                _ => return Some(msg),
            }
        }

        Some(msg)
    }
}

fn encode_varint(mut val: u64, buf: &mut Vec<u8>) {
    while val >= 0x80 {
        buf.push(((val & 0x7F) | 0x80) as u8);
        val >>= 7;
    }
    buf.push(val as u8);
}

fn encode_bytes(data: &[u8], buf: &mut Vec<u8>) {
    encode_varint(data.len() as u64, buf);
    buf.extend_from_slice(data);
}

fn decode_varint(bytes: &[u8]) -> Option<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift = 0;
    for (i, &b) in bytes.iter().enumerate() {
        result |= ((b & 0x7F) as u64) << shift;
        if (b & 0x80) == 0 {
            return Some((result, i + 1));
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

fn decode_bytes(bytes: &[u8]) -> Option<(&[u8], usize)> {
    let (len, n) = decode_varint(bytes)?;
    let len = len as usize;
    if bytes.len() < n + len {
        return None;
    }
    Some((&bytes[n..n + len], n + len))
}

/// mDNS discovery helper to find Google Cast devices on the LAN
pub fn discover_cast_device(target_name: Option<&str>, timeout: Duration) -> Option<(String, u16)> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.set_read_timeout(Some(timeout)).ok()?;
    socket.set_broadcast(true).ok()?;

    // Standard DNS query for _googlecast._tcp.local (PTR record, IN class)
    let query: &[u8] = &[
        0x12, 0x34, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 11, b'_', b'g',
        b'o', b'o', b'g', b'l', b'e', b'c', b'a', b's', b't', 4, b'_', b't', b'c', b'p', 5, b'l',
        b'o', b'c', b'a', b'l', 0, 0x00, 0x0c, // Type: PTR
        0x00, 0x01, // Class: IN
    ];

    let mcast_addr: SocketAddr = "224.0.0.251:5353".parse().ok()?;
    let _ = socket.send_to(query, mcast_addr);

    let start = Instant::now();
    let mut buf = [0u8; 4096];

    while start.elapsed() < timeout {
        if let Ok((len, src)) = socket.recv_from(&mut buf) {
            let data = &buf[..len];
            let data_str = String::from_utf8_lossy(data);

            if let Some(name) = target_name {
                if !name.is_empty() && !data_str.to_lowercase().contains(&name.to_lowercase()) {
                    continue;
                }
            }

            let ip = src.ip().to_string();
            return Some((ip, 8009));
        }
    }

    None
}

/// Client for polling Cast devices over TLS (port 8009)
pub struct GoogleCastClient {
    device_ip: String,
    device_port: u16,
    request_id: u32,
}

impl GoogleCastClient {
    pub fn new(device_ip: &str, device_port: u16) -> Self {
        Self {
            device_ip: device_ip.to_string(),
            device_port: device_port.max(1),
            request_id: 1,
        }
    }

    pub fn poll_status(&mut self) -> Result<CastMediaStatus, String> {
        if self.device_ip.is_empty() {
            return Err("No device IP specified".to_string());
        }

        // Establish TCP connection
        let addr = format!("{}:{}", self.device_ip, self.device_port);
        let stream = TcpStream::connect_timeout(
            &addr.parse().map_err(|e| format!("Invalid addr: {}", e))?,
            Duration::from_millis(1500),
        )
        .map_err(|e| format!("TCP connect failed: {}", e))?;

        stream
            .set_read_timeout(Some(Duration::from_millis(2000)))
            .ok();
        stream
            .set_write_timeout(Some(Duration::from_millis(2000)))
            .ok();

        // Wrap with rustls client config ignoring self-signed cast certs
        let root_store = rustls::RootCertStore::empty();
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let mut config = config;
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(NoCertificateVerification));

        let server_name = "cast.local".try_into().map_err(|_| "Invalid server name")?;
        let mut tls_conn = rustls::ClientConnection::new(Arc::new(config), server_name)
            .map_err(|e| format!("TLS init error: {}", e))?;

        let mut sock = stream;
        let mut tls_stream = rustls::Stream::new(&mut tls_conn, &mut sock);

        // 1. Send CONNECT to receiver-0
        self.send_msg(
            &mut tls_stream,
            "sender-0",
            "receiver-0",
            NS_CONNECTION,
            r#"{"type":"CONNECT"}"#,
        )?;

        // 2. Send GET_STATUS to receiver-0
        self.request_id += 1;
        let get_status = format!(r#"{{"type":"GET_STATUS","requestId":{}}}"#, self.request_id);
        self.send_msg(
            &mut tls_stream,
            "sender-0",
            "receiver-0",
            NS_RECEIVER,
            &get_status,
        )?;

        let mut status = CastMediaStatus::default();
        let mut transport_id = None;
        let mut app_name = String::new();

        // Read receiver status responses
        for _ in 0..6 {
            if let Ok(msg) = self.recv_msg(&mut tls_stream) {
                if msg.namespace == NS_HEARTBEAT && msg.payload_utf8.contains("PING") {
                    let _ = self.send_msg(
                        &mut tls_stream,
                        "sender-0",
                        "receiver-0",
                        NS_HEARTBEAT,
                        r#"{"type":"PONG"}"#,
                    );
                } else if msg.namespace == NS_RECEIVER {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&msg.payload_utf8) {
                        if let Some(volume) = json.pointer("/status/volume") {
                            status.volume_level =
                                volume.get("level").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
                            status.is_muted = volume
                                .get("muted")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                        }
                        if let Some(apps) = json
                            .pointer("/status/applications")
                            .and_then(|a| a.as_array())
                        {
                            if let Some(app) = apps.first() {
                                app_name = app
                                    .get("displayName")
                                    .and_then(|d| d.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                if let Some(t_id) = app.get("transportId").and_then(|t| t.as_str())
                                {
                                    transport_id = Some(t_id.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        status.app_name = app_name;

        // If an active media session transport exists, query its MEDIA_STATUS
        if let Some(tid) = transport_id {
            let _ = self.send_msg(
                &mut tls_stream,
                "sender-0",
                &tid,
                NS_CONNECTION,
                r#"{"type":"CONNECT"}"#,
            );
            self.request_id += 1;
            let get_media_status =
                format!(r#"{{"type":"GET_STATUS","requestId":{}}}"#, self.request_id);
            let _ = self.send_msg(
                &mut tls_stream,
                "sender-0",
                &tid,
                NS_MEDIA,
                &get_media_status,
            );

            for _ in 0..6 {
                if let Ok(msg) = self.recv_msg(&mut tls_stream) {
                    if msg.namespace == NS_MEDIA {
                        if let Ok(json) =
                            serde_json::from_str::<serde_json::Value>(&msg.payload_utf8)
                        {
                            if let Some(statuses) =
                                json.pointer("/status").and_then(|s| s.as_array())
                            {
                                if let Some(media_stat) = statuses.first() {
                                    status.is_active = true;
                                    let player_state = media_stat
                                        .get("playerState")
                                        .and_then(|p| p.as_str())
                                        .unwrap_or("");
                                    status.is_playing =
                                        player_state == "PLAYING" || player_state == "BUFFERING";
                                    status.current_time_sec = media_stat
                                        .get("currentTime")
                                        .and_then(|c| c.as_f64())
                                        .unwrap_or(0.0)
                                        as f32;

                                    if let Some(media) = media_stat.get("media") {
                                        status.duration_sec = media
                                            .get("duration")
                                            .and_then(|d| d.as_f64())
                                            .unwrap_or(0.0)
                                            as f32;
                                        if let Some(meta) = media.get("metadata") {
                                            status.title = meta
                                                .get("title")
                                                .and_then(|t| t.as_str())
                                                .unwrap_or("")
                                                .to_string();
                                            status.artist = meta
                                                .get("artist")
                                                .or_else(|| meta.get("subtitle"))
                                                .and_then(|a| a.as_str())
                                                .unwrap_or("")
                                                .to_string();
                                            status.album = meta
                                                .get("albumName")
                                                .and_then(|a| a.as_str())
                                                .unwrap_or("")
                                                .to_string();

                                            if let Some(images) =
                                                meta.get("images").and_then(|i| i.as_array())
                                            {
                                                if let Some(img) = images.first() {
                                                    status.image_url = img
                                                        .get("url")
                                                        .and_then(|u| u.as_str())
                                                        .map(|s| s.to_string());
                                                }
                                            }
                                        }
                                    }
                                    status.last_updated = Some(Instant::now());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(status)
    }

    fn send_msg<S: Read + Write>(
        &self,
        stream: &mut S,
        source: &str,
        dest: &str,
        ns: &str,
        payload: &str,
    ) -> Result<(), String> {
        let msg = CastMessage::new(source, dest, ns, payload);
        let bytes = msg.encode();
        let len = (bytes.len() as u32).to_be_bytes();

        stream
            .write_all(&len)
            .map_err(|e| format!("Write len error: {}", e))?;
        stream
            .write_all(&bytes)
            .map_err(|e| format!("Write payload error: {}", e))?;
        stream.flush().map_err(|e| format!("Flush error: {}", e))?;
        Ok(())
    }

    fn recv_msg<S: Read + Write>(&self, stream: &mut S) -> Result<CastMessage, String> {
        let mut len_buf = [0u8; 4];
        stream
            .read_exact(&mut len_buf)
            .map_err(|e| format!("Read len error: {}", e))?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > 65536 {
            return Err("Frame too large".to_string());
        }

        let mut payload_buf = vec![0u8; len];
        stream
            .read_exact(&mut payload_buf)
            .map_err(|e| format!("Read payload error: {}", e))?;

        CastMessage::decode(&payload_buf).ok_or_else(|| "Failed to decode CastMessage".to_string())
    }
}

// Certificate verifier that allows self-signed Google Cast certificates
#[derive(Debug)]
struct NoCertificateVerification;

impl rustls::client::danger::ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cast_message_encode_decode() {
        let msg = CastMessage::new(
            "sender-0",
            "receiver-0",
            NS_CONNECTION,
            r#"{"type":"CONNECT"}"#,
        );
        let encoded = msg.encode();
        assert!(!encoded.is_empty());

        let decoded = CastMessage::decode(&encoded).expect("Should decode CastMessage");
        assert_eq!(decoded.source_id, "sender-0");
        assert_eq!(decoded.destination_id, "receiver-0");
        assert_eq!(decoded.namespace, NS_CONNECTION);
        assert_eq!(decoded.payload_utf8, r#"{"type":"CONNECT"}"#);
    }
}
