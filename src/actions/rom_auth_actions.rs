//! Pure helpers for linking a game ROM to a Discord account (code -> `rom_` token).
#![allow(dead_code)] // consumed by later tasks (handlers)

use rand::rngs::OsRng;
use rand::{Rng, RngCore};
use sha2::{Digest, Sha256};

pub const ROM_TOKEN_PREFIX: &str = "rom_";
pub const CODE_TTL_MINUTES: i64 = 15;

/// Generates a 6-character linking code, each character in `1..=9`.
pub fn generate_code() -> String {
    (0..6)
        .map(|_| char::from(b'0' + OsRng.gen_range(1..=9u8)))
        .collect()
}

/// A valid code is exactly 6 characters, each in `1..=9`.
pub fn valid_code(code: &str) -> bool {
    code.len() == 6 && code.bytes().all(|c| (b'1'..=b'9').contains(&c))
}

/// Generates `rom_` + 64 lowercase hex chars (32 random bytes).
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let mut token = String::from(ROM_TOKEN_PREFIX);
    for b in bytes {
        token.push_str(&format!("{b:02x}"));
    }
    token
}

/// Lowercase hex SHA-256 of the full token (prefix included).
pub fn hash_token(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Extracts the full `rom_...` token from a raw `Authorization` header value.
pub fn parse_rom_bearer(value: &str) -> Option<&str> {
    let token = value.strip_prefix("Bearer ")?;
    let hex = token.strip_prefix(ROM_TOKEN_PREFIX)?;
    let ok = hex.len() == 64 && hex.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'));
    ok.then_some(token)
}

/// Trims, removes control characters and truncates to 32 characters.
pub fn sanitize_label(label: Option<&str>) -> String {
    label
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_six_digits_from_one_to_nine() {
        for _ in 0..200 {
            let code = generate_code();
            assert_eq!(code.len(), 6);
            assert!(code.bytes().all(|c| (b'1'..=b'9').contains(&c)));
            assert!(valid_code(&code));
        }
        for bad in ["", "12345", "1234567", "123450", "12a456", " 12345"] {
            assert!(!valid_code(bad));
        }
    }

    #[test]
    fn tokens_are_prefixed_random_hex_and_hashed() {
        let token = generate_token();
        assert!(token.starts_with("rom_"));
        assert_eq!(token.len(), 4 + 64);
        assert_ne!(token, generate_token());
        let hash = hash_token(&token);
        assert_eq!(hash.len(), 64);
        assert_ne!(hash, token[4..]);
        assert_eq!(hash, hash_token(&token));
    }

    #[test]
    fn rom_bearer_requires_exact_format() {
        let token = generate_token();
        assert_eq!(parse_rom_bearer(&format!("Bearer {token}")), Some(token.as_str()));
        for bad in [token.clone(), format!("bearer {token}"), format!("Bearer {}", token.to_uppercase()),
                    format!("Bearer {}x", token), "Bearer rom_abc".into(), format!("Bearer {}", &token[4..])] {
            assert_eq!(parse_rom_bearer(&bad), None, "{bad}");
        }
    }

    #[test]
    fn labels_are_trimmed_cleaned_and_bounded() {
        assert_eq!(sanitize_label(None), "");
        assert_eq!(sanitize_label(Some("  Console salon  ")), "Console salon");
        assert_eq!(sanitize_label(Some("a\nb\u{7}c")), "abc");
        assert_eq!(sanitize_label(Some(&"é".repeat(40))).chars().count(), 32);
    }
}
