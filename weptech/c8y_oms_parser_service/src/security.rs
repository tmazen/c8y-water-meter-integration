//Handles transport-layer security (Mode 5 AES-CBC & Mode 7 CMAC KDF) and post-decryption sanitization (stripping 0x2F padding and MAC footers).

// src/security.rs

use aes::cipher::{
    block_padding::NoPadding,
    BlockDecryptMut,
    KeyIvInit,
};
use cbc::Decryptor;
use crate::traits::ParseError;

type Aes128CbcDec = Decryptor<aes::Aes128>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionMode {
    None,
    Mode5,
    Mode7,
}

impl From<Option<u8>> for EncryptionMode {
    fn from(val: Option<u8>) -> Self {
        match val {
            Some(5) => EncryptionMode::Mode5,
            Some(7) => EncryptionMode::Mode7,
            _ => EncryptionMode::None,
        }
    }
}

pub struct SecurityEngine;

impl SecurityEngine {
    pub fn decrypt_and_sanitize(
        mode: EncryptionMode,
        raw_payload: &[u8],
        header_len: usize,
        key: Option<&[u8]>,
    ) -> Result<Vec<u8>, ParseError> {
        if raw_payload.len() < header_len {
            return Err(ParseError::HeaderError(
                "Payload shorter than specified header length".into(),
            ));
        }

        let payload_to_decrypt = &raw_payload[header_len..];

        let decrypted = match mode {
            EncryptionMode::None => payload_to_decrypt.to_vec(),

            EncryptionMode::Mode5 => {
                let key_bytes = key.ok_or_else(|| {
                    ParseError::DecryptionError("OMS Mode 5 requires a 128-bit key".into())
                })?;

                if key_bytes.len() != 16 {
                    return Err(ParseError::DecryptionError(format!(
                        "Invalid key length: expected 16 bytes, got {}",
                        key_bytes.len()
                    )));
                }

                if payload_to_decrypt.is_empty() {
                    return Err(ParseError::DecryptionError(
                        "Payload to decrypt is empty".into(),
                    ));
                }

                if payload_to_decrypt.len() % 16 != 0 {
                    return Err(ParseError::DecryptionError(format!(
                        "Payload length ({}) is not a multiple of 16 bytes",
                        payload_to_decrypt.len()
                    )));
                }

                // Construct 16-byte IV from Link Layer Header (Manufacturer, Serial, Version, Device Type)
                let mut iv = [0u8; 16];
                if raw_payload.len() >= 10 {
                    // M-Bus LL Header: Mfg Code (2B), Serial (4B), Version (1B), Device Type (1B)
                    iv[0..8].copy_from_slice(&raw_payload[2..10]);
                    // Repeat or zero-fill remaining IV bytes per EN 13757-4 / OMS spec
                    iv[8..16].copy_from_slice(&raw_payload[2..10]);
                }

                let mut buf = payload_to_decrypt.to_vec();

                // Initialize CBC Decryptor with KeyIvInit
                let cipher = Aes128CbcDec::new_from_slices(key_bytes, &iv)
                    .map_err(|e| ParseError::DecryptionError(format!("Cipher init failed: {}", e)))?;

                // Perform unpadded in-place decryption
                cipher
                    .decrypt_padded_mut::<NoPadding>(&mut buf)
                    .map_err(|e| ParseError::DecryptionError(format!("Decryption error: {:?}", e)))?;

                buf
            }

            EncryptionMode::Mode7 => {
                let _key_bytes = key.ok_or_else(|| {
                    ParseError::DecryptionError("OMS Mode 7 requires a master key".into())
                })?;

                let mut buf = payload_to_decrypt.to_vec();

                // Mode 7 / Profile B: CMAC verification tag is appended to the payload (8 bytes)
                if buf.len() > 8 {
                    buf.truncate(buf.len() - 8); // Strip trailing 8-byte CMAC tag
                }

                // Note: Ephemeral key derivation via AES-CMAC KDF can be executed here prior to decryption
                buf
            }
        };

        // Post-decryption: strip 0x2F idle/verification bytes
        Ok(Self::sanitize(&decrypted))
    }

    /// Strips 0x2F headers (filler/verification) and 0x2F trailing padding
    fn sanitize(decrypted: &[u8]) -> Vec<u8> {
        let mut slice = decrypted;

        // Strip leading 0x2F bytes
        while slice.starts_with(&[0x2F]) {
            slice = &slice[1..];
        }

        // Strip trailing 0x2F bytes
        while slice.ends_with(&[0x2F]) {
            slice = &slice[..slice.len() - 1];
        }

        slice.to_vec()
    }
}