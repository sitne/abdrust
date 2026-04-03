use crate::state::VoiceCapabilities;

pub const ENGINE_NAME: &str = "davey";
pub const MAX_DAVE_PROTOCOL_VERSION: u16 = davey::DAVE_PROTOCOL_VERSION;
pub const DAVE_FRAME_MARKER_BYTES: [u8; 2] = [0xFA, 0xFA];

pub const fn voice_capabilities(
    engine_name: &'static str,
    supports_dave: bool,
    max_dave_protocol_version: u16,
) -> VoiceCapabilities {
    VoiceCapabilities {
        engine_name,
        supports_dave,
        max_dave_protocol_version: if supports_dave {
            max_dave_protocol_version
        } else {
            0
        },
    }
}

pub fn is_dave_required_error(err: &anyhow::Error) -> bool {
    let text = err.to_string();
    text.contains("4017") || text.contains("DAVE") || text.contains("E2EE")
}

pub fn dave_join_banner(engine_name: &str, max_dave_protocol_version: u16) -> String {
    format!("{} / DAVE v{}", engine_name, max_dave_protocol_version)
}

pub fn packet_has_dave_marker(packet: &[u8]) -> bool {
    packet.ends_with(&DAVE_FRAME_MARKER_BYTES)
}
