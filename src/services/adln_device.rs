//! ADLN device credentials: random IDs and secrets, and the AES-256-GCM
//! envelope shared with the game server (`secret_ciphertext` in `adln_devices`).

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::rngs::OsRng;
use rand::{Rng, RngCore};
use std::fmt;

/// Environment variable of the game server's 32-byte wrapping key (64 hex characters).
pub const WRAPPING_KEY_ENV: &str = "ADLN_DEVICE_WRAPPING_KEY";
const AAD_LABEL: &[u8] = b"ADLN/device-secret/1";
const ENVELOPE_PREFIX: &str = "v1.";
/// `"v1." + base64url(12 + 32 + 16 bytes)`.
pub const ENVELOPE_LEN: usize = 83;
const ID_ALPHABET: &[u8; 36] = b"abcdefghijklmnopqrstuvwxyz0123456789";

/// The shared device wrapping key. Never printed, zeroed on drop.
pub struct WrappingKey([u8; 32]);

impl WrappingKey {
    /// Parses exactly 64 hexadecimal characters (surrounding whitespace ignored).
    pub fn from_hex(value: &str) -> Option<Self> {
        let value = value.trim().as_bytes();
        if value.len() != 64 {
            return None;
        }
        let mut key = [0u8; 32];
        for (byte, pair) in key.iter_mut().zip(value.chunks(2)) {
            let pair = std::str::from_utf8(pair).ok()?;
            *byte = u8::from_str_radix(pair, 16).ok()?;
        }
        Some(Self(key))
    }

    /// Reads [`WRAPPING_KEY_ENV`]; the error never contains the value.
    pub fn from_env() -> Result<Self, &'static str> {
        let value =
            std::env::var(WRAPPING_KEY_ENV).map_err(|_| "ADLN_DEVICE_WRAPPING_KEY is not set")?;
        Self::from_hex(&value).ok_or("ADLN_DEVICE_WRAPPING_KEY must be 64 hexadecimal characters")
    }

    pub(crate) fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for WrappingKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WrappingKey(<redacted>)")
    }
}

impl Drop for WrappingKey {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            // Volatile so the wipe is not optimised away.
            unsafe { std::ptr::write_volatile(byte, 0) };
        }
    }
}

/// A fresh public device ID: 15 lowercase letters/digits from the OS CSPRNG.
pub fn generate_device_id() -> String {
    (0..crate::services::mkds_save::RECORD_ID_LEN)
        .map(|_| char::from(ID_ALPHABET[OsRng.gen_range(0..ID_ALPHABET.len())]))
        .collect()
}

/// A fresh 32-byte device secret from the OS CSPRNG.
pub fn generate_secret() -> [u8; 32] {
    let mut secret = [0u8; 32];
    OsRng.fill_bytes(&mut secret);
    secret
}

/// `UTF8("ADLN/device-secret/1") || 0 || device_id || 0 || player_record_id`.
fn associated_data(device_id: &str, player_id: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(AAD_LABEL.len() + 2 + device_id.len() + player_id.len());
    aad.extend_from_slice(AAD_LABEL);
    aad.push(0);
    aad.extend_from_slice(device_id.as_bytes());
    aad.push(0);
    aad.extend_from_slice(player_id.as_bytes());
    aad
}

fn seal_with_nonce(
    key: &WrappingKey,
    device_id: &str,
    player_id: &str,
    secret: &[u8; 32],
    nonce: [u8; 12],
) -> String {
    let cipher = Aes256Gcm::new_from_slice(key.bytes()).expect("32-byte key");
    let aad = associated_data(device_id, player_id);
    let sealed = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: secret,
                aad: &aad,
            },
        )
        .expect("AES-GCM encryption of 32 bytes cannot fail");
    let mut envelope = Vec::with_capacity(60);
    envelope.extend_from_slice(&nonce);
    envelope.extend_from_slice(&sealed);
    format!("{ENVELOPE_PREFIX}{}", URL_SAFE_NO_PAD.encode(envelope))
}

/// Encrypts a device secret for `adln_devices.secret_ciphertext`, with a fresh random nonce.
pub fn seal(key: &WrappingKey, device_id: &str, player_id: &str, secret: &[u8; 32]) -> String {
    let mut nonce = [0u8; 12];
    OsRng.fill_bytes(&mut nonce);
    seal_with_nonce(key, device_id, player_id, secret, nonce)
}

/// Decrypts and authenticates a stored envelope for this exact device and player.
pub fn open(
    key: &WrappingKey,
    device_id: &str,
    player_id: &str,
    envelope: &str,
) -> Option<[u8; 32]> {
    if envelope.len() != ENVELOPE_LEN {
        return None;
    }
    let raw = URL_SAFE_NO_PAD
        .decode(envelope.strip_prefix(ENVELOPE_PREFIX)?)
        .ok()?;
    if raw.len() != 60 {
        return None;
    }
    let cipher = Aes256Gcm::new_from_slice(key.bytes()).ok()?;
    let aad = associated_data(device_id, player_id);
    let secret = cipher
        .decrypt(
            Nonce::from_slice(&raw[..12]),
            Payload {
                msg: &raw[12..],
                aad: &aad,
            },
        )
        .ok()?;
    secret.try_into().ok()
}

/// Constant-time comparison of two secrets.
pub fn secrets_equal(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY_HEX: &str = "030a11181f262d343b424950575e656c737a81888f969da4abb2b9c0c7ced5dc";
    /// Produced independently with Node's `crypto.createCipheriv("aes-256-gcm")`
    /// (nonce a0..ab, secret ff..e0, device abcdefghij12345, player 0s90t9j1i8zxpwr).
    const REFERENCE_ENVELOPE: &str =
        "v1.oKGio6SlpqeoqaqrhHqO9RaHVi3jcN-k2e0dsPTA0oJ7DQuMwh78t8HUyWy8yAYVddWwDxDrAanTZUAE";
    const DEVICE: &str = "abcdefghij12345";
    const PLAYER: &str = "0s90t9j1i8zxpwr";

    fn reference_secret() -> [u8; 32] {
        std::array::from_fn(|i| 255 - i as u8)
    }

    #[test]
    fn envelope_matches_the_shared_aes_gcm_format() {
        let key = WrappingKey::from_hex(KEY_HEX).unwrap();
        let nonce = std::array::from_fn(|i| 0xa0 + i as u8);
        let envelope = seal_with_nonce(&key, DEVICE, PLAYER, &reference_secret(), nonce);
        assert_eq!(envelope, REFERENCE_ENVELOPE);
        assert_eq!(envelope.len(), ENVELOPE_LEN);
        assert_eq!(
            open(&key, DEVICE, PLAYER, REFERENCE_ENVELOPE),
            Some(reference_secret())
        );
    }

    #[test]
    fn envelope_is_bound_to_the_key_device_and_player() {
        let key = WrappingKey::from_hex(KEY_HEX).unwrap();
        let other_key = WrappingKey::from_hex(&"11".repeat(32)).unwrap();
        assert_eq!(open(&other_key, DEVICE, PLAYER, REFERENCE_ENVELOPE), None);
        assert_eq!(
            open(&key, "zbcdefghij12345", PLAYER, REFERENCE_ENVELOPE),
            None
        );
        assert_eq!(
            open(&key, DEVICE, "1s90t9j1i8zxpwr", REFERENCE_ENVELOPE),
            None
        );
        // No trailing NUL after the player ID.
        assert_eq!(
            open(&key, DEVICE, &format!("{PLAYER}\0"), REFERENCE_ENVELOPE),
            None
        );
        let mut tampered = REFERENCE_ENVELOPE.as_bytes().to_vec();
        tampered[40] = if tampered[40] == b'A' { b'B' } else { b'A' };
        assert_eq!(
            open(
                &key,
                DEVICE,
                PLAYER,
                std::str::from_utf8(&tampered).unwrap()
            ),
            None
        );
        assert_eq!(open(&key, DEVICE, PLAYER, "v2.x"), None);
    }

    #[test]
    fn fresh_nonces_ids_and_secrets_are_random() {
        let key = WrappingKey::from_hex(KEY_HEX).unwrap();
        let secret = generate_secret();
        let first = seal(&key, DEVICE, PLAYER, &secret);
        let second = seal(&key, DEVICE, PLAYER, &secret);
        assert_ne!(first, second, "each encryption uses a new nonce");
        assert_eq!(open(&key, DEVICE, PLAYER, &first), Some(secret));
        assert_ne!(generate_secret(), generate_secret());
        let id = generate_device_id();
        assert!(crate::services::mkds_save::valid_record_id(&id), "{id}");
        assert_ne!(id, generate_device_id());
    }

    #[test]
    fn keys_must_be_64_hex_characters_and_never_print() {
        assert!(WrappingKey::from_hex(&"ab".repeat(31)).is_none());
        assert!(WrappingKey::from_hex(&"zz".repeat(32)).is_none());
        assert!(WrappingKey::from_hex(&"é".repeat(32)).is_none());
        let key = WrappingKey::from_hex(&format!(" {KEY_HEX}\n")).unwrap();
        assert_eq!(format!("{key:?}"), "WrappingKey(<redacted>)");
        assert!(secrets_equal(&[1; 32], &[1; 32]));
        assert!(!secrets_equal(&[1; 32], &[2; 32]));
    }
}
