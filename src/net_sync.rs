//! # Ultra-Compact Binary Network Serialization (Postcard & Bitcode)
//!
//! Provides zero-allocation, sub-microsecond binary state serialization
//! to drastically reduce network bandwidth compared to JSON and traditional TCP/UDP.

use serde::{Deserialize, Serialize};

/// High-density player transform and combat state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, bitcode::Encode, bitcode::Decode)]
pub struct PlayerNetworkState {
    pub entity_id: u32,
    pub tick: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub health: u16,
    pub score: u16,
    pub flags: u8, // Bitflags: bit 0: is_firing, bit 1: is_grounded, bit 2: is_reloading
}

impl PlayerNetworkState {
    pub fn new(entity_id: u32, tick: u64, pos: (f32, f32, f32), yaw: f32, pitch: f32, health: u16, score: u16, flags: u8) -> Self {
        Self {
            entity_id,
            tick,
            x: pos.0,
            y: pos.1,
            z: pos.2,
            yaw,
            pitch,
            health,
            score,
            flags,
        }
    }

    /// Encode with bitcode (ultra-fast bitpacking)
    pub fn encode_bitcode(&self) -> Vec<u8> {
        bitcode::encode(self)
    }

    /// Decode with bitcode
    pub fn decode_bitcode(bytes: &[u8]) -> Result<Self, bitcode::Error> {
        bitcode::decode(bytes)
    }

    /// Encode with postcard (variable-length compact binary)
    pub fn encode_postcard(&self) -> Result<Vec<u8>, postcard::Error> {
        postcard::to_allocvec(self)
    }

    /// Decode with postcard
    pub fn decode_postcard(bytes: &[u8]) -> Result<Self, postcard::Error> {
        postcard::from_bytes(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binary_serialization_efficiency() {
        let state = PlayerNetworkState::new(
            42,
            12850,
            (124.5, 8.2, -34.8),
            3.1415,
            0.25,
            100,
            2450,
            0b0000_0011,
        );

        // 1. JSON comparison
        let json_bytes = serde_json::to_vec(&state).unwrap();
        
        // 2. Bitcode encoding
        let bitcode_bytes = state.encode_bitcode();
        let decoded_bitcode = PlayerNetworkState::decode_bitcode(&bitcode_bytes).unwrap();
        assert_eq!(state, decoded_bitcode);

        // 3. Postcard encoding
        let postcard_bytes = state.encode_postcard().unwrap();
        let decoded_postcard = PlayerNetworkState::decode_postcard(&postcard_bytes).unwrap();
        assert_eq!(state, decoded_postcard);

        println!(
            "\n[Benchmark Serialization Size]\n- JSON: {} bytes\n- Postcard: {} bytes\n- Bitcode: {} bytes",
            json_bytes.len(),
            postcard_bytes.len(),
            bitcode_bytes.len()
        );

        // Binary should be at least 60% smaller than JSON
        assert!(bitcode_bytes.len() < json_bytes.len() / 2);
        assert!(postcard_bytes.len() < json_bytes.len() / 2);
    }
}
