use anyhow::{Context, Result};
use davey::{DaveSession, MediaType, SessionStatus, SigningKeyPair};
use std::num::NonZeroU16;
use tracing::{debug, info};

/// Wrapper around davey::DaveSession for voice gateway use
pub struct DaveyVoiceSession {
    session: DaveSession,
    user_id: u64,
    channel_id: u64,
    signing_key: SigningKeyPair,
}

impl DaveyVoiceSession {
    /// Create a new DAVE session
    pub fn new(user_id: u64, channel_id: u64, protocol_version: u16) -> Result<Self> {
        let signing_key = SigningKeyPair::generate();
        let protocol_version =
            NonZeroU16::new(protocol_version).context("protocol version must be non-zero")?;

        let session = DaveSession::new(protocol_version, user_id, channel_id, Some(&signing_key))
            .context("failed to create DaveSession")?;

        info!(
            "Created DAVE session: user_id={}, channel_id={}, protocol_version={}",
            user_id, channel_id, protocol_version
        );

        Ok(Self {
            session,
            user_id,
            channel_id,
            signing_key,
        })
    }

    /// Reinitialize the session
    pub fn reinit(&mut self, protocol_version: u16) -> Result<()> {
        let protocol_version =
            NonZeroU16::new(protocol_version).context("protocol version must be non-zero")?;

        self.session
            .reinit(
                protocol_version,
                self.user_id,
                self.channel_id,
                Some(&self.signing_key),
            )
            .context("failed to reinit DAVE session")?;

        info!(
            "Reinitialized DAVE session: protocol_version={}",
            protocol_version
        );
        Ok(())
    }

    /// Reset the session
    pub fn reset(&mut self) -> Result<()> {
        self.session
            .reset()
            .context("failed to reset DAVE session")?;
        debug!("Reset DAVE session");
        Ok(())
    }

    /// Set external sender from opcode 25
    pub fn set_external_sender(&mut self, external_sender_data: &[u8]) -> Result<()> {
        self.session
            .set_external_sender(external_sender_data)
            .context("failed to set external sender")?;
        debug!("Set external sender");
        Ok(())
    }

    /// Create a key package for opcode 26
    pub fn create_key_package(&mut self) -> Result<Vec<u8>> {
        let kp = self
            .session
            .create_key_package()
            .context("failed to create key package")?;
        debug!("Created key package: {} bytes", kp.len());
        Ok(kp)
    }

    /// Process proposals from opcode 27
    /// Returns Some((commit, welcome)) if a commit should be sent
    pub fn process_proposals(
        &mut self,
        operation_type: u8,
        proposals: &[u8],
    ) -> Result<Option<(Vec<u8>, Option<Vec<u8>>)>> {
        let op_type = match operation_type {
            0 => davey::ProposalsOperationType::APPEND,
            1 => davey::ProposalsOperationType::REVOKE,
            _ => anyhow::bail!("unknown proposals operation type: {}", operation_type),
        };

        let result = self
            .session
            .process_proposals(op_type, proposals, None)
            .context("failed to process proposals")?;

        match result {
            Some(commit_welcome) => {
                debug!(
                    "Processed proposals: commit={} bytes, welcome={} bytes",
                    commit_welcome.commit.len(),
                    commit_welcome
                        .welcome
                        .as_ref()
                        .map(|w| w.len())
                        .unwrap_or(0)
                );
                Ok(Some((commit_welcome.commit, commit_welcome.welcome)))
            }
            None => {
                debug!("Processed proposals: no commit needed");
                Ok(None)
            }
        }
    }

    /// Process welcome from opcode 30
    pub fn process_welcome(&mut self, welcome: &[u8]) -> Result<()> {
        self.session
            .process_welcome(welcome)
            .context("failed to process welcome")?;
        info!("Processed DAVE welcome message");
        Ok(())
    }

    /// Process commit from opcode 29
    pub fn process_commit(&mut self, commit: &[u8]) -> Result<()> {
        self.session
            .process_commit(commit)
            .context("failed to process commit")?;
        info!("Processed DAVE commit message");
        Ok(())
    }

    /// Check if session is ready for encryption/decryption
    pub fn is_ready(&self) -> bool {
        self.session.is_ready()
    }

    /// Get session status
    pub fn status(&self) -> SessionStatus {
        self.session.status()
    }

    /// Get protocol version
    pub fn protocol_version(&self) -> u16 {
        self.session.protocol_version().get()
    }

    /// Get user IDs in the group
    pub fn get_user_ids(&self) -> Option<Vec<u64>> {
        self.session.get_user_ids()
    }

    /// Get voice privacy code
    pub fn voice_privacy_code(&self) -> Option<&str> {
        self.session.voice_privacy_code()
    }

    /// Encrypt an Opus packet
    pub fn encrypt_opus(&mut self, packet: &[u8]) -> Result<Vec<u8>> {
        if !self.session.is_ready() {
            anyhow::bail!("DAVE session not ready for encryption");
        }

        let encrypted = self
            .session
            .encrypt_opus(packet)
            .context("failed to encrypt opus packet")?;

        Ok(encrypted.into_owned())
    }

    /// Decrypt a packet for a specific user
    pub fn decrypt(&mut self, user_id: u64, packet: &[u8]) -> Result<Vec<u8>> {
        let decrypted = self
            .session
            .decrypt(user_id, MediaType::AUDIO, packet)
            .context("failed to decrypt packet")?;

        Ok(decrypted)
    }

    /// Check if passthrough mode is available for a user
    pub fn can_passthrough(&self, user_id: u64) -> bool {
        self.session.can_passthrough(user_id)
    }

    /// Set passthrough mode on all decryptors
    pub fn set_passthrough_mode(&mut self, enabled: bool) {
        self.session.set_passthrough_mode(enabled, None);
    }

    /// Get the inner session reference
    pub fn inner(&self) -> &DaveSession {
        &self.session
    }

    /// Get the inner session mutable reference
    pub fn inner_mut(&mut self) -> &mut DaveSession {
        &mut self.session
    }
}
