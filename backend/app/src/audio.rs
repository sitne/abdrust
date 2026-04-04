/// A source of audio data that can be played over voice.
pub trait AudioSource: Send + Sync {
    /// Get the next packet of audio data.
    /// Returns `None` when the source is exhausted.
    fn next_packet(&mut self) -> Option<Vec<i16>>;

    /// Whether this source is stereo.
    /// The voice engine will downmix to mono if needed.
    fn is_stereo(&self) -> bool {
        false
    }

    /// Whether this source has more data.
    fn is_done(&self) -> bool {
        false
    }
}

// ============================================================
// Built-in implementations
// ============================================================

/// Silence source — produces infinite zero samples.
pub struct Silence;

impl AudioSource for Silence {
    fn next_packet(&mut self) -> Option<Vec<i16>> {
        // 20ms of silence at 48kHz mono = 960 samples
        Some(vec![0i16; 960])
    }

    fn is_done(&self) -> bool {
        false // Silence never ends
    }
}

/// PCM buffer source — plays a fixed buffer of PCM samples once.
pub struct PcmSource {
    samples: Vec<i16>,
    position: usize,
    frame_size: usize,
}

impl PcmSource {
    /// Create a new PCM source with the given samples.
    /// Frames are emitted in chunks of `frame_size` samples (default 960 = 20ms).
    pub fn new(samples: Vec<i16>) -> Self {
        Self {
            samples,
            position: 0,
            frame_size: 960,
        }
    }

    /// Set the frame size in samples.
    pub fn with_frame_size(mut self, frame_size: usize) -> Self {
        self.frame_size = frame_size;
        self
    }
}

impl AudioSource for PcmSource {
    fn next_packet(&mut self) -> Option<Vec<i16>> {
        if self.position >= self.samples.len() {
            return None;
        }

        let end = (self.position + self.frame_size).min(self.samples.len());
        let frame = self.samples[self.position..end].to_vec();
        self.position = end;
        Some(frame)
    }

    fn is_done(&self) -> bool {
        self.position >= self.samples.len()
    }
}

/// Raw Opus packet source — plays pre-encoded Opus packets.
pub struct OpusSource {
    packets: Vec<Vec<u8>>,
    position: usize,
}

impl OpusSource {
    /// Create a new Opus source from pre-encoded packets.
    pub fn new(packets: Vec<Vec<u8>>) -> Self {
        Self {
            packets,
            position: 0,
        }
    }

    /// Get the raw Opus packets (for direct transmission without re-encoding).
    pub fn into_packets(self) -> Vec<Vec<u8>> {
        self.packets
    }
}

impl AudioSource for OpusSource {
    fn next_packet(&mut self) -> Option<Vec<i16>> {
        // OpusSource returns packets as-is; the voice engine
        // will transmit them directly without re-encoding.
        // We return None here to signal that this source
        // uses a different transmission path.
        if self.position >= self.packets.len() {
            return None;
        }
        // Return a placeholder; the actual Opus data is
        // transmitted via a separate mechanism.
        self.position += 1;
        Some(vec![])
    }

    fn is_done(&self) -> bool {
        self.position >= self.packets.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_silence_never_ends() {
        let mut silence = Silence;
        assert!(!silence.is_done());
        let packet = silence.next_packet();
        assert!(packet.is_some());
        assert_eq!(packet.unwrap().len(), 960);
        assert!(!silence.is_done());
    }

    #[test]
    fn test_pcm_source_single_frame() {
        let samples: Vec<i16> = (0..960).map(|i| i as i16).collect();
        let mut source = PcmSource::new(samples);

        let packet = source.next_packet().unwrap();
        assert_eq!(packet.len(), 960);
        assert!(source.is_done());
        assert!(source.next_packet().is_none());
    }

    #[test]
    fn test_pcm_source_multiple_frames() {
        let samples: Vec<i16> = (0..2880).map(|i| i as i16).collect();
        let mut source = PcmSource::new(samples).with_frame_size(960);

        let p1 = source.next_packet().unwrap();
        assert_eq!(p1.len(), 960);
        assert!(!source.is_done());

        let p2 = source.next_packet().unwrap();
        assert_eq!(p2.len(), 960);
        assert!(!source.is_done());

        let p3 = source.next_packet().unwrap();
        assert_eq!(p3.len(), 960);
        assert!(source.is_done());

        assert!(source.next_packet().is_none());
    }

    #[test]
    fn test_pcm_source_partial_frame() {
        let samples: Vec<i16> = vec![1, 2, 3];
        let mut source = PcmSource::new(samples);

        let packet = source.next_packet().unwrap();
        assert_eq!(packet, vec![1, 2, 3]);
        assert!(source.is_done());
    }

    #[test]
    fn test_opus_source() {
        let packets = vec![vec![0xF8, 0xFF, 0xFE], vec![0xF8, 0xFF, 0xFE]];
        let mut source = OpusSource::new(packets);

        assert!(!source.is_done());
        source.next_packet();
        assert!(!source.is_done());
        source.next_packet();
        assert!(source.is_done());
        assert!(source.next_packet().is_none());
    }
}
