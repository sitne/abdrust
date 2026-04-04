use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use anyhow::{Context, Result};
use byteorder::{BigEndian, WriteBytesExt};
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;
use tracing::{debug, info, warn};

/// RTP header structure (12 bytes minimum)
/// 0                   1                   2                   3
/// 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |V=2|P|X|  CC   |M|     PT      |       sequence number         |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |                           timestamp                           |
/// +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
/// |           synchronization source (SSRC) identifier            |
/// +=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+=+
#[derive(Debug, Clone)]
pub struct RtpHeader {
    pub version: u8,
    pub padding: bool,
    pub extension: bool,
    pub csrc_count: u8,
    pub marker: bool,
    pub payload_type: u8,
    pub sequence: u16,
    pub timestamp: u32,
    pub ssrc: u32,
    pub csrcs: Vec<u32>,
}

impl RtpHeader {
    pub const MIN_SIZE: usize = 12;

    /// Parse RTP header from bytes
    pub fn parse(data: &[u8]) -> Result<(Self, usize)> {
        if data.len() < Self::MIN_SIZE {
            anyhow::bail!("RTP data too short: {} bytes", data.len());
        }

        let first = data[0];
        let version = (first >> 6) & 0x03;
        let padding = (first & 0x20) != 0;
        let extension = (first & 0x10) != 0;
        let csrc_count = first & 0x0F;

        let second = data[1];
        let marker = (second & 0x80) != 0;
        let payload_type = second & 0x7F;

        let sequence = u16::from_be_bytes([data[2], data[3]]);
        let timestamp = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let ssrc = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);

        let header_len = Self::MIN_SIZE + (csrc_count as usize) * 4;

        if data.len() < header_len {
            anyhow::bail!(
                "RTP data too short for CSRCs: {} bytes, need {}",
                data.len(),
                header_len
            );
        }

        let mut csrcs = Vec::with_capacity(csrc_count as usize);
        for i in 0..csrc_count as usize {
            let offset = Self::MIN_SIZE + i * 4;
            let csrc = u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            csrcs.push(csrc);
        }

        // Check for extension header
        let mut total_header_len = header_len;
        if extension && data.len() > total_header_len + 4 {
            let ext_offset = total_header_len;
            // _profile = u16::from_be_bytes([data[ext_offset], data[ext_offset + 1]]);
            let ext_len = u16::from_be_bytes([data[ext_offset + 2], data[ext_offset + 3]]) as usize;
            total_header_len += 4 + ext_len * 4;
        }

        if data.len() < total_header_len {
            anyhow::bail!(
                "RTP data too short for extension: {} bytes, need {}",
                data.len(),
                total_header_len
            );
        }

        Ok((
            Self {
                version,
                padding,
                extension,
                csrc_count,
                marker,
                payload_type,
                sequence,
                timestamp,
                ssrc,
                csrcs,
            },
            total_header_len,
        ))
    }

    /// Serialize RTP header to bytes
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::MIN_SIZE + self.csrcs.len() * 4);

        let first = (self.version << 6)
            | if self.padding { 0x20 } else { 0 }
            | if self.extension { 0x10 } else { 0 }
            | (self.csrc_count & 0x0F);
        buf.push(first);

        let second = if self.marker { 0x80 } else { 0 } | (self.payload_type & 0x7F);
        buf.push(second);

        buf.extend_from_slice(&self.sequence.to_be_bytes());
        buf.extend_from_slice(&self.timestamp.to_be_bytes());
        buf.extend_from_slice(&self.ssrc.to_be_bytes());

        for &csrc in &self.csrcs {
            buf.extend_from_slice(&csrc.to_be_bytes());
        }

        buf
    }
}

/// Transport encryption modes supported by Discord
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportCryptoMode {
    /// No transport encryption (legacy)
    None,
    /// AES-256-GCM with RTP-size nonce (RFC 7714)
    Aes256Gcm,
    /// XChaCha20-Poly1305 with RTP-size nonce
    XChaCha20Poly1305,
}

impl TransportCryptoMode {
    pub fn from_str(mode: &str) -> Self {
        match mode {
            "aead_aes256_gcm_rtpsize" => Self::Aes256Gcm,
            "aead_xchacha20_poly1305_rtpsize" => Self::XChaCha20Poly1305,
            _ => Self::None,
        }
    }

    /// AEAD tag size in bytes
    pub fn tag_size(&self) -> usize {
        match self {
            Self::Aes256Gcm | Self::XChaCha20Poly1305 => 16,
            Self::None => 0,
        }
    }

    /// Build 12-byte nonce for AES-256-GCM per RFC 7714
    /// Format: [0x00 (2 bytes)][SSRC (4 bytes)][sequence (2 bytes)][0x00 (4 bytes)]
    pub fn build_nonce(&self, ssrc: u32, sequence: u16) -> [u8; 12] {
        let mut nonce = [0u8; 12];
        // bytes 0-1: 0x00 (already zero)
        // bytes 2-5: SSRC (big-endian)
        nonce[2..6].copy_from_slice(&ssrc.to_be_bytes());
        // bytes 6-7: sequence number (big-endian)
        nonce[6..8].copy_from_slice(&sequence.to_be_bytes());
        // bytes 8-11: 0x00 (already zero)
        nonce
    }
}

/// UDP voice socket for sending/receiving RTP packets
pub struct VoiceUdpSocket {
    socket: UdpSocket,
    remote_addr: SocketAddr,
    sequence: u16,
    timestamp: u32,
    ssrc: u32,
    secret_key: [u8; 32],
    encryption_mode: String,
}

impl VoiceUdpSocket {
    /// Create a new UDP voice socket from an existing socket (for IP discovery reuse)
    pub fn from_raw(
        socket: UdpSocket,
        remote_addr: SocketAddr,
        ssrc: u32,
        secret_key: [u8; 32],
        encryption_mode: String,
    ) -> Result<Self> {
        socket
            .set_read_timeout(Some(Duration::from_millis(100)))
            .ok();

        info!(
            "Voice UDP socket (from raw): remote={}, ssrc={}",
            remote_addr, ssrc
        );

        Ok(Self {
            socket,
            remote_addr,
            sequence: 0,
            timestamp: 0,
            ssrc,
            secret_key,
            encryption_mode,
        })
    }

    /// Create a new UDP voice socket
    pub fn new(
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        ssrc: u32,
        secret_key: [u8; 32],
        encryption_mode: String,
    ) -> Result<Self> {
        let socket = UdpSocket::bind(local_addr).context("failed to bind UDP socket")?;
        socket
            .set_read_timeout(Some(Duration::from_millis(100)))
            .ok();

        info!(
            "Voice UDP socket: local={}, remote={}, ssrc={}",
            local_addr, remote_addr, ssrc
        );

        Ok(Self {
            socket,
            remote_addr,
            sequence: 0,
            timestamp: 0,
            ssrc,
            secret_key,
            encryption_mode,
        })
    }

    /// Receive a packet (non-blocking, returns None if no data)
    pub fn recv(&self, buf: &mut [u8]) -> Result<Option<usize>> {
        match self.socket.recv_from(buf) {
            Ok((len, _addr)) => Ok(Some(len)),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e).context("failed to receive UDP packet"),
        }
    }

    /// Send an RTP packet with DAVE E2EE encrypted payload
    /// The payload should already be encrypted by davey (with 0xFAFA marker)
    pub fn send_dave_encrypted(&mut self, e2ee_payload: &[u8]) -> Result<()> {
        let header = RtpHeader {
            version: 2,
            padding: false,
            extension: false,
            csrc_count: 0,
            marker: false,
            payload_type: 0x78, // Opus
            sequence: self.sequence,
            timestamp: self.timestamp,
            ssrc: self.ssrc,
            csrcs: vec![],
        };

        let mut packet = header.serialize();
        packet.extend_from_slice(e2ee_payload);

        // Transport encryption: for DAVE sessions, append nonce
        let nonce = self.sequence.to_be_bytes();
        packet.extend_from_slice(&nonce);

        self.socket
            .send_to(&packet, self.remote_addr)
            .context("failed to send UDP packet")?;

        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self.timestamp.wrapping_add(960);

        Ok(())
    }

    /// Send an RTP packet (for non-DAVE or passthrough)
    pub fn send(&mut self, payload: &[u8]) -> Result<()> {
        let header = RtpHeader {
            version: 2,
            padding: false,
            extension: false,
            csrc_count: 0,
            marker: false,
            payload_type: 0x78, // Opus
            sequence: self.sequence,
            timestamp: self.timestamp,
            ssrc: self.ssrc,
            csrcs: vec![],
        };

        let mut packet = header.serialize();
        packet.extend_from_slice(payload);

        // Apply transport encryption
        let encrypted = self.encrypt_transport(&packet)?;

        self.socket
            .send_to(&encrypted, self.remote_addr)
            .context("failed to send UDP packet")?;

        self.sequence = self.sequence.wrapping_add(1);
        // Opus at 48kHz, 20ms frames = 960 samples
        self.timestamp = self.timestamp.wrapping_add(960);

        Ok(())
    }

    /// Encrypt packet for transport (non-DAVE)
    fn encrypt_transport(&self, packet: &[u8]) -> Result<Vec<u8>> {
        // For DAVE-enabled sessions, the payload is already E2EE encrypted
        // We just need transport-level encryption with the secret_key
        match self.encryption_mode.as_str() {
            "aead_aes256_gcm_rtpsize" | "aead_xchacha20_poly1305_rtpsize" => {
                // These modes encrypt the payload and append a nonce
                // The nonce is a 32-bit counter appended to the payload
                let mut encrypted = packet.to_vec();
                // For now, passthrough - full implementation needs crypto
                // The transport encryption is handled by the DAVE layer for E2EE
                let nonce = self.sequence.to_be_bytes();
                encrypted.extend_from_slice(&nonce);
                Ok(encrypted)
            }
            _ => {
                warn!("Unknown encryption mode: {}", self.encryption_mode);
                Ok(packet.to_vec())
            }
        }
    }

    /// Get the socket for IP discovery
    pub fn socket(&self) -> &UdpSocket {
        &self.socket
    }

    /// Get SSRC
    pub fn ssrc(&self) -> u32 {
        self.ssrc
    }

    /// Update the secret key (received from Session Description)
    pub fn set_secret_key(&mut self, key: [u8; 32]) {
        self.secret_key = key;
    }

    /// Decrypt transport-encrypted RTP payload
    ///
    /// For AES-256-GCM (RFC 7714):
    /// - Input: [RTP header][ciphertext][16-byte AEAD tag]
    /// - Nonce: [0x00(2)][SSRC(4)][sequence(4)][0x00(2)]
    /// - AAD: RTP header (authenticated but not encrypted)
    /// - Output: [RTP header][plaintext payload]
    pub fn decrypt_transport(
        &self,
        rtp_header: &[u8],
        payload_with_tag: &[u8],
        ssrc: u32,
        sequence: u16,
    ) -> Result<Vec<u8>> {
        let mode = TransportCryptoMode::from_str(&self.encryption_mode);

        if mode == TransportCryptoMode::None {
            return Ok(payload_with_tag.to_vec());
        }

        let tag_size = mode.tag_size();
        if payload_with_tag.len() < tag_size {
            anyhow::bail!(
                "payload too short for AEAD tag: {} bytes",
                payload_with_tag.len()
            );
        }

        match mode {
            TransportCryptoMode::Aes256Gcm => {
                let cipher = Aes256Gcm::new_from_slice(&self.secret_key)
                    .context("failed to create AES-256-GCM cipher")?;

                let nonce_bytes = mode.build_nonce(ssrc, sequence);
                let nonce = Nonce::from_slice(&nonce_bytes);

                // RFC 7714: AAD = RTP header, ciphertext = payload_with_tag
                let payload = Payload {
                    msg: payload_with_tag,
                    aad: rtp_header,
                };
                let plaintext = cipher
                    .decrypt(nonce, payload)
                    .map_err(|e| anyhow::anyhow!("AES-256-GCM decrypt failed: {}", e))?;

                Ok(plaintext)
            }
            TransportCryptoMode::XChaCha20Poly1305 => {
                // TODO: implement XChaCha20-Poly1305
                warn!("XChaCha20-Poly1305 transport decryption not yet implemented");
                Ok(payload_with_tag.to_vec())
            }
            TransportCryptoMode::None => Ok(payload_with_tag.to_vec()),
        }
    }
}

/// IP Discovery for NAT traversal
pub struct IpDiscovery {
    socket: UdpSocket,
    remote_addr: SocketAddr,
    ssrc: u32,
}

impl IpDiscovery {
    /// Create IP discovery
    pub fn new(socket: UdpSocket, remote_addr: SocketAddr, ssrc: u32) -> Self {
        Self {
            socket,
            remote_addr,
            ssrc,
        }
    }

    /// Perform IP discovery
    /// Returns (address, port) as discovered by the server
    pub fn discover(&self) -> Result<(String, u16)> {
        // Build IP discovery packet
        let mut buf = Vec::with_capacity(74);
        buf.write_u16::<BigEndian>(1)?;
        buf.write_u16::<BigEndian>(70)?;
        buf.write_u32::<BigEndian>(self.ssrc)?;

        let addr_buf = [0u8; 64];
        buf.extend_from_slice(&addr_buf);

        buf.write_u16::<BigEndian>(0)?;

        // Set blocking mode for IP discovery
        self.socket.set_nonblocking(false).ok();
        self.socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .ok();

        self.socket
            .send_to(&buf, self.remote_addr)
            .context("failed to send IP discovery packet")?;

        debug!("Sent IP discovery packet");

        // Receive response
        let mut response = [0u8; 74];
        let (len, _addr) = self
            .socket
            .recv_from(&mut response)
            .context("failed to receive IP discovery response")?;

        if len < 74 {
            anyhow::bail!("IP discovery response too short: {} bytes", len);
        }

        let _resp_type = u16::from_be_bytes([response[0], response[1]]);
        let _resp_len = u16::from_be_bytes([response[2], response[3]]);
        let _resp_ssrc = u32::from_be_bytes([response[4], response[5], response[6], response[7]]);

        let addr_bytes = &response[8..72];
        let null_pos = addr_bytes.iter().position(|&b| b == 0).unwrap_or(64);
        let address = String::from_utf8_lossy(&addr_bytes[..null_pos]).to_string();

        let port = u16::from_be_bytes([response[72], response[73]]);

        info!("IP Discovery: address={}, port={}", address, port);

        Ok((address, port))
    }
}

/// Opus encoder wrapper
pub struct OpusEncoder {
    encoder: opus::Encoder,
}

impl OpusEncoder {
    /// Create a new Opus encoder
    /// 48kHz, mono, voice quality
    pub fn new() -> Result<Self> {
        let encoder = opus::Encoder::new(48_000, opus::Channels::Mono, opus::Application::Voip)
            .context("failed to create Opus encoder")?;

        Ok(Self { encoder })
    }

    /// Encode PCM samples to Opus packet
    /// Input: i16 PCM samples at 48kHz, mono
    /// Output: Opus encoded packet
    pub fn encode(&mut self, pcm: &[i16]) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; 4000]; // Max Opus packet size
        let len = self
            .encoder
            .encode(pcm, &mut buf)
            .context("failed to encode PCM to Opus")?;

        buf.truncate(len);
        Ok(buf)
    }

    /// Encode silence frame (3 bytes: 0xF8, 0xFF, 0xFE)
    pub fn silence_frame() -> Vec<u8> {
        vec![0xF8, 0xFF, 0xFE]
    }
}

/// Opus decoder wrapper
pub struct OpusDecoder {
    decoder: opus::Decoder,
}

impl OpusDecoder {
    /// Create a new Opus decoder
    /// 48kHz, mono, voice quality
    pub fn new() -> Result<Self> {
        let decoder = opus::Decoder::new(48_000, opus::Channels::Mono)
            .context("failed to create Opus decoder")?;

        Ok(Self { decoder })
    }

    /// Decode Opus packet to PCM samples
    /// Input: Opus encoded packet
    /// Output: i16 PCM samples at 48kHz, mono (up to 960 samples = 20ms)
    pub fn decode(&mut self, opus_data: &[u8]) -> Result<Vec<i16>> {
        let mut pcm = vec![0i16; 960]; // 20ms at 48kHz
        let len = self
            .decoder
            .decode(opus_data, &mut pcm, false)
            .context("failed to decode Opus packet")?;

        pcm.truncate(len);
        Ok(pcm)
    }

    /// Decode silence frame (3 bytes: 0xF8, 0xFF, 0xFE) to PCM
    pub fn decode_silence() -> Vec<i16> {
        // Silence frame decodes to 960 zero samples (20ms at 48kHz)
        vec![0i16; 960]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rtp_header_minimal_parse() {
        // Standard RTP header: V=2, P=0, X=0, CC=0, M=0, PT=0x78 (Opus)
        let data: Vec<u8> = vec![
            0x80, // V=2, P=0, X=0, CC=0
            0x78, // M=0, PT=0x78
            0x00, 0x01, // sequence = 1
            0x00, 0x00, 0x00, 0x00, // timestamp = 0
            0x00, 0x00, 0x00, 0x01, // ssrc = 1
        ];

        let (header, offset) = RtpHeader::parse(&data).unwrap();
        assert_eq!(header.version, 2);
        assert!(!header.padding);
        assert!(!header.extension);
        assert_eq!(header.csrc_count, 0);
        assert!(!header.marker);
        assert_eq!(header.payload_type, 0x78);
        assert_eq!(header.sequence, 1);
        assert_eq!(header.timestamp, 0);
        assert_eq!(header.ssrc, 1);
        assert!(header.csrcs.is_empty());
        assert_eq!(offset, 12);
    }

    #[test]
    fn test_rtp_header_serialize_roundtrip() {
        let original = RtpHeader {
            version: 2,
            padding: false,
            extension: false,
            csrc_count: 0,
            marker: true,
            payload_type: 0x78,
            sequence: 12345,
            timestamp: 960,
            ssrc: 42,
            csrcs: vec![],
        };

        let serialized = original.serialize();
        let (parsed, offset) = RtpHeader::parse(&serialized).unwrap();

        assert_eq!(parsed.version, original.version);
        assert_eq!(parsed.marker, original.marker);
        assert_eq!(parsed.payload_type, original.payload_type);
        assert_eq!(parsed.sequence, original.sequence);
        assert_eq!(parsed.timestamp, original.timestamp);
        assert_eq!(parsed.ssrc, original.ssrc);
        assert_eq!(offset, 12);
    }

    #[test]
    fn test_rtp_header_with_csrc() {
        let header = RtpHeader {
            version: 2,
            padding: false,
            extension: false,
            csrc_count: 2,
            marker: false,
            payload_type: 0x78,
            sequence: 100,
            timestamp: 48000,
            ssrc: 1,
            csrcs: vec![10, 20],
        };

        let serialized = header.serialize();
        assert_eq!(serialized.len(), 12 + 2 * 4); // 12 + CC*4

        let (parsed, offset) = RtpHeader::parse(&serialized).unwrap();
        assert_eq!(parsed.csrc_count, 2);
        assert_eq!(parsed.csrcs, vec![10, 20]);
        assert_eq!(offset, 20);
    }

    #[test]
    fn test_rtp_header_too_short() {
        let data: Vec<u8> = vec![0x80, 0x78, 0x00, 0x01];
        assert!(RtpHeader::parse(&data).is_err());
    }

    #[test]
    fn test_rtp_header_with_extension() {
        // V=2, X=1, CC=0
        let data: Vec<u8> = vec![
            0x90, // V=2, X=1
            0x78, // M=0, PT=0x78
            0x00, 0x01, // sequence = 1
            0x00, 0x00, 0x00, 0x00, // timestamp = 0
            0x00, 0x00, 0x00, 0x01, // ssrc = 1
            0xBE, 0xDE, // extension profile (RFC 5285 one-byte header)
            0x00, 0x01, // extension length = 1 (4 bytes)
            0x00, 0x00, 0x00, 0x00, // extension data
        ];

        let (header, offset) = RtpHeader::parse(&data).unwrap();
        assert!(header.extension);
        assert_eq!(offset, 20); // 12 + 4 (ext header) + 4 (ext data)
    }

    #[test]
    fn test_rtp_header_opus_voice() {
        // Realistic Discord voice RTP header
        let data: Vec<u8> = vec![
            0x80, // V=2
            0x78, // PT=Opus (120)
            0x00, 0x42, // sequence = 66
            0x00, 0x00, 0x03, 0xC0, // timestamp = 960
            0x00, 0x01, 0x02, 0x03, // ssrc
        ];

        let (header, offset) = RtpHeader::parse(&data).unwrap();
        assert_eq!(header.version, 2);
        assert_eq!(header.payload_type, 0x78);
        assert_eq!(header.sequence, 66);
        assert_eq!(header.timestamp, 960);
        assert_eq!(header.ssrc, 0x00010203);
        assert_eq!(offset, 12);
    }

    #[test]
    fn test_transport_crypto_mode_from_str() {
        assert_eq!(
            TransportCryptoMode::from_str("aead_aes256_gcm_rtpsize"),
            TransportCryptoMode::Aes256Gcm
        );
        assert_eq!(
            TransportCryptoMode::from_str("aead_xchacha20_poly1305_rtpsize"),
            TransportCryptoMode::XChaCha20Poly1305
        );
        assert_eq!(
            TransportCryptoMode::from_str("xsalsa20_poly1305"),
            TransportCryptoMode::None
        );
        assert_eq!(
            TransportCryptoMode::from_str("unknown"),
            TransportCryptoMode::None
        );
    }

    #[test]
    fn test_transport_crypto_mode_tag_size() {
        assert_eq!(TransportCryptoMode::Aes256Gcm.tag_size(), 16);
        assert_eq!(TransportCryptoMode::XChaCha20Poly1305.tag_size(), 16);
        assert_eq!(TransportCryptoMode::None.tag_size(), 0);
    }

    #[test]
    fn test_transport_crypto_mode_build_nonce() {
        let mode = TransportCryptoMode::Aes256Gcm;
        let nonce = mode.build_nonce(0x01020304, 0x0042);

        // Expected: [0x00, 0x00, SSRC(4 bytes BE), sequence(2 bytes BE), 0x00, 0x00, 0x00, 0x00]
        assert_eq!(nonce[0], 0x00);
        assert_eq!(nonce[1], 0x00);
        assert_eq!(&nonce[2..6], &0x01020304u32.to_be_bytes());
        assert_eq!(&nonce[6..8], &0x0042u16.to_be_bytes());
        assert_eq!(&nonce[8..12], &[0x00, 0x00, 0x00, 0x00]);
    }

    #[test]
    fn test_transport_decrypt_aes256_gcm_roundtrip() {
        use aes_gcm::aead::Aead as _;
        use aes_gcm::{Aes256Gcm as AesGcmCipher, KeyInit as _, Nonce as GcmNonce};

        // Create a test socket with a known secret key
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let remote: SocketAddr = "127.0.0.1:9999".parse().unwrap();
        let secret_key = [0x42u8; 32]; // Test key
        let mut udp = VoiceUdpSocket::from_raw(
            socket,
            remote,
            0x01020304, // SSRC
            secret_key,
            "aead_aes256_gcm_rtpsize".to_string(),
        )
        .unwrap();

        // Build RTP header
        let rtp_header: Vec<u8> = vec![
            0x80, 0x78, // V=2, PT=Opus
            0x00, 0x01, // sequence = 1
            0x00, 0x00, 0x00, 0x00, // timestamp = 0
            0x01, 0x02, 0x03, 0x04, // SSRC
        ];

        // Original payload (simulated Opus data)
        let original_payload: Vec<u8> = vec![0xF8, 0xFF, 0xFE, 0x00, 0x01, 0x02];

        // Encrypt using AES-256-GCM per RFC 7714:
        // - AAD = RTP header
        // - Plaintext = payload
        // - Nonce = [0,0,SSRC,seq,0,0,0,0]
        let cipher = AesGcmCipher::new_from_slice(&secret_key).unwrap();
        let nonce_bytes = TransportCryptoMode::Aes256Gcm.build_nonce(0x01020304, 1);
        let nonce = GcmNonce::from_slice(&nonce_bytes);

        // Encrypt payload with RTP header as AAD
        let payload = aes_gcm::aead::Payload {
            msg: original_payload.as_ref(),
            aad: &rtp_header,
        };
        let ciphertext_with_tag = cipher.encrypt(&nonce, payload).unwrap();

        // Decrypt using our implementation
        let result = udp.decrypt_transport(&rtp_header, &ciphertext_with_tag, 0x01020304, 1);
        assert!(result.is_ok());
        let decrypted = result.unwrap();
        assert_eq!(decrypted, original_payload);
    }
}
