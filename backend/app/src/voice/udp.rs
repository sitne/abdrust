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
        // Type: u16 (1 = request), Length: u16 (70), SSRC: u32, Address: [u8; 64], Port: u16
        let mut buf = Vec::with_capacity(74);
        buf.write_u16::<BigEndian>(1)?; // Type: request
        buf.write_u16::<BigEndian>(70)?; // Length
        buf.write_u32::<BigEndian>(self.ssrc)?; // SSRC

        // 64 bytes for address (null-terminated)
        let addr_buf = [0u8; 64];
        buf.extend_from_slice(&addr_buf);

        // 2 bytes for port
        buf.write_u16::<BigEndian>(0)?;

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

        // Parse response
        // Type: u16 (should be 2), Length: u16 (70), SSRC: u32, Address: [u8; 64], Port: u16
        let _resp_type = u16::from_be_bytes([response[0], response[1]]);
        let _resp_len = u16::from_be_bytes([response[2], response[3]]);
        let _resp_ssrc = u32::from_be_bytes([response[4], response[5], response[6], response[7]]);

        // Address is null-terminated string
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
