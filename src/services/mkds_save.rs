//! ADL device credential pages inside a raw Mario Kart DS (EUR) save.
//!
//! The save is untrusted input. Only the two reserved 256-byte pages at
//! `0x3E000` and `0x3E100` are ever written; every other byte, native game
//! sections and checksums included, is copied unchanged.

use std::fmt;
use std::ops::Range;

/// Exact size of a raw MKDS save (`.sav`), without any emulator container.
pub const SAVE_LEN: usize = 0x40000;
/// First eight bytes of a MKDS save.
pub const SAVE_MAGIC: &[u8; 8] = b"MKDSSV10";
pub const PAGE_A: usize = 0x3E000;
pub const PAGE_B: usize = 0x3E100;
pub const PAGE_LEN: usize = 0x100;
/// The only bytes a credential write may change (half-open).
pub const CREDENTIAL_REGION: Range<usize> = PAGE_A..PAGE_B + PAGE_LEN;

const PAGE_MAGIC: &[u8; 4] = b"ADLC";
const GAME: &[u8; 4] = b"MKDS";
const VERSION: u16 = 1;
const RECORD_LEN: u16 = 128;
const CRC_OFFSET: usize = 0x7C;
/// Length of a PocketBase record ID (device and player).
pub const RECORD_ID_LEN: usize = 15;

/// A PocketBase record ID as stored in a page: 15 lowercase ASCII letters or digits.
pub fn valid_record_id(id: &str) -> bool {
    id.len() == RECORD_ID_LEN
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// One device credential, as written in both pages.
#[derive(Clone, PartialEq, Eq)]
pub struct Credential {
    pub generation: u64,
    pub login_counter: u64,
    pub device_id: String,
    pub player_id: String,
    pub secret: [u8; 32],
}

impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credential")
            .field("generation", &self.generation)
            .field("login_counter", &self.login_counter)
            .field("device_id", &self.device_id)
            .field("player_id", &self.player_id)
            .field("secret", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveError {
    /// Wrong size or magic: not a raw MKDS save.
    InvalidSave,
    /// Nonempty reserved pages that are not a supported ADL credential.
    UnsupportedCredential,
    /// Two valid copies that disagree.
    ConflictingCopies,
    /// No valid copy left to recover from.
    DamagedCopies,
}

#[derive(Debug, PartialEq, Eq)]
enum Page {
    Empty,
    Valid(Credential),
    Damaged,
}

/// Accepts only a raw save: exact length and `MKDSSV10` magic.
pub fn validate_save(save: &[u8]) -> Result<(), SaveError> {
    if save.len() == SAVE_LEN && save.starts_with(SAVE_MAGIC) {
        Ok(())
    } else {
        Err(SaveError::InvalidSave)
    }
}

fn read_id(field: &[u8]) -> Option<String> {
    let (id, nul) = field.split_at(RECORD_ID_LEN);
    let id = std::str::from_utf8(id).ok()?;
    (nul == [0] && valid_record_id(id)).then(|| id.to_owned())
}

fn id_field(id: &str) -> [u8; 16] {
    let mut field = [0u8; 16];
    field[..id.len()].copy_from_slice(id.as_bytes());
    field
}

fn u16_at(page: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([page[offset], page[offset + 1]])
}

fn u32_at(page: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(page[offset..offset + 4].try_into().unwrap())
}

fn u64_at(page: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(page[offset..offset + 8].try_into().unwrap())
}

/// Classifies one page. A page announcing `ADLC` version 1 whose checksum or
/// padding is wrong is damaged; anything else that is neither empty nor a
/// conforming page is unsupported and rejected.
fn decode_page(page: &[u8]) -> Result<Page, SaveError> {
    if page.iter().all(|&b| b == 0) || page.iter().all(|&b| b == 0xFF) {
        return Ok(Page::Empty);
    }
    if &page[0..4] != PAGE_MAGIC || u16_at(page, 0x04) != VERSION {
        return Err(SaveError::UnsupportedCredential);
    }
    if crc32fast::hash(&page[..CRC_OFFSET]) != u32_at(page, CRC_OFFSET)
        || page[0x80..].iter().any(|&b| b != 0)
    {
        return Ok(Page::Damaged);
    }
    // Intact (checksum matches) yet nonconforming: an unknown variant, not damage.
    if u16_at(page, 0x06) != RECORD_LEN
        || &page[0x08..0x0C] != GAME
        || u32_at(page, 0x0C) != 0
        || page[0x60..CRC_OFFSET].iter().any(|&b| b != 0)
    {
        return Err(SaveError::UnsupportedCredential);
    }
    let (Some(device_id), Some(player_id)) =
        (read_id(&page[0x20..0x30]), read_id(&page[0x30..0x40]))
    else {
        return Err(SaveError::UnsupportedCredential);
    };
    Ok(Page::Valid(Credential {
        generation: u64_at(page, 0x10),
        login_counter: u64_at(page, 0x18),
        device_id,
        player_id,
        secret: page[0x40..0x60].try_into().unwrap(),
    }))
}

/// Encodes one 256-byte page (same bytes as the reference `struct.pack` layout).
pub fn encode_page(credential: &Credential) -> [u8; PAGE_LEN] {
    assert!(valid_record_id(&credential.device_id) && valid_record_id(&credential.player_id));
    let mut page = [0u8; PAGE_LEN];
    page[0x00..0x04].copy_from_slice(PAGE_MAGIC);
    page[0x04..0x06].copy_from_slice(&VERSION.to_le_bytes());
    page[0x06..0x08].copy_from_slice(&RECORD_LEN.to_le_bytes());
    page[0x08..0x0C].copy_from_slice(GAME);
    // 0x0C flags stay 0.
    page[0x10..0x18].copy_from_slice(&credential.generation.to_le_bytes());
    page[0x18..0x20].copy_from_slice(&credential.login_counter.to_le_bytes());
    page[0x20..0x30].copy_from_slice(&id_field(&credential.device_id));
    page[0x30..0x40].copy_from_slice(&id_field(&credential.player_id));
    page[0x40..0x60].copy_from_slice(&credential.secret);
    // 0x60..0x7C reserved and 0x80..0x100 padding stay 0.
    let crc = crc32fast::hash(&page[..CRC_OFFSET]);
    page[CRC_OFFSET..0x80].copy_from_slice(&crc.to_le_bytes());
    page
}

/// Reads the credential stored in a validated save.
///
/// `Ok(None)` means both pages are empty. Two valid copies must agree on the
/// device, player and secret; the highest generation wins, and equal
/// generations must be identical. A damaged (or empty) copy is recovered from
/// a valid mirror; two unusable copies are rejected.
pub fn read_credential(save: &[u8]) -> Result<Option<Credential>, SaveError> {
    validate_save(save)?;
    let a = decode_page(&save[PAGE_A..PAGE_A + PAGE_LEN])?;
    let b = decode_page(&save[PAGE_B..PAGE_B + PAGE_LEN])?;
    match (a, b) {
        (Page::Empty, Page::Empty) => Ok(None),
        (Page::Valid(a), Page::Valid(b)) => {
            if a.device_id != b.device_id || a.player_id != b.player_id || a.secret != b.secret {
                return Err(SaveError::ConflictingCopies);
            }
            match a.generation.cmp(&b.generation) {
                std::cmp::Ordering::Greater => Ok(Some(a)),
                std::cmp::Ordering::Less => Ok(Some(b)),
                std::cmp::Ordering::Equal if a == b => Ok(Some(a)),
                std::cmp::Ordering::Equal => Err(SaveError::ConflictingCopies),
            }
        }
        (Page::Valid(valid), _) | (_, Page::Valid(valid)) => Ok(Some(valid)),
        _ => Err(SaveError::DamagedCopies),
    }
}

/// Returns a separate copy of `original` with `credential` in both pages.
pub fn write_credential(original: &[u8], credential: &Credential) -> Result<Vec<u8>, SaveError> {
    validate_save(original)?;
    let page = encode_page(credential);
    let mut patched = original.to_vec();
    patched[PAGE_A..PAGE_A + PAGE_LEN].copy_from_slice(&page);
    patched[PAGE_B..PAGE_B + PAGE_LEN].copy_from_slice(&page);
    Ok(patched)
}

/// Final check before a download: same length, every byte outside the
/// credential region unchanged, and both pages decoding to `expected`.
pub fn verify_patched(
    original: &[u8],
    patched: &[u8],
    expected: &Credential,
) -> Result<(), String> {
    if original.len() != SAVE_LEN || patched.len() != original.len() {
        return Err("patched save length changed".into());
    }
    if original[..CREDENTIAL_REGION.start] != patched[..CREDENTIAL_REGION.start]
        || original[CREDENTIAL_REGION.end..] != patched[CREDENTIAL_REGION.end..]
    {
        return Err("patched save changed bytes outside the credential pages".into());
    }
    let page = encode_page(expected);
    if patched[PAGE_A..PAGE_A + PAGE_LEN] != page || patched[PAGE_B..PAGE_B + PAGE_LEN] != page {
        return Err("credential pages differ from the expected encoding".into());
    }
    match read_credential(patched) {
        Ok(Some(read)) if &read == expected => Ok(()),
        _ => Err("credential pages do not decode to the expected credential".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference page computed with the documented Python `struct.pack` + `zlib.crc32`
    /// (generation 7, login counter 42, secret 00..1f).
    const REFERENCE_PAGE: &str = "41444c43010080004d4b44530000000007000000000000002a000000000000006162636465666768696a3132333435003073393074396a3169387a7870777200000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f00000000000000000000000000000000000000000000000000000000cc8b3136";

    fn hex(bytes: &str) -> Vec<u8> {
        (0..bytes.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&bytes[i..i + 2], 16).unwrap())
            .collect()
    }

    pub(crate) fn credential(generation: u64) -> Credential {
        Credential {
            generation,
            login_counter: 0,
            device_id: "abcdefghij12345".into(),
            player_id: "0s90t9j1i8zxpwr".into(),
            secret: std::array::from_fn(|i| i as u8),
        }
    }

    /// A deterministic, noisy save: every byte differs from its neighbours.
    pub(crate) fn sample_save() -> Vec<u8> {
        let mut save: Vec<u8> = (0..SAVE_LEN)
            .map(|i| (i as u32).wrapping_mul(2_654_435_761).to_le_bytes()[1])
            .collect();
        save[..8].copy_from_slice(SAVE_MAGIC);
        save[CREDENTIAL_REGION].fill(0xFF);
        save
    }

    #[test]
    fn page_matches_the_reference_layout_and_crc() {
        let page = encode_page(&Credential {
            login_counter: 42,
            ..credential(7)
        });
        let mut expected = hex(REFERENCE_PAGE);
        expected.resize(PAGE_LEN, 0);
        assert_eq!(page.to_vec(), expected);
        assert_eq!(u32_at(&page, CRC_OFFSET), 0x3631_8bcc);
        assert_eq!(&page[0x2F..0x30], [0]);
        assert_eq!(&page[0x3F..0x40], [0]);
    }

    #[test]
    fn only_raw_mkds_saves_are_accepted() {
        let save = sample_save();
        assert_eq!(validate_save(&save), Ok(()));
        let mut truncated = save.clone();
        truncated.pop();
        let mut appended = save.clone();
        appended.extend_from_slice(b"|-DESMUME SAVE-|");
        let mut wrong_magic = save.clone();
        wrong_magic[..8].copy_from_slice(b"MKDSSV11");
        for bad in [truncated, appended, wrong_magic, vec![]] {
            assert_eq!(validate_save(&bad), Err(SaveError::InvalidSave));
            assert_eq!(read_credential(&bad), Err(SaveError::InvalidSave));
        }
    }

    #[test]
    fn writes_identical_mirrors_and_preserves_every_other_byte() {
        let original = sample_save();
        let snapshot = original.clone();
        let credential = credential(1);
        let patched = write_credential(&original, &credential).unwrap();
        assert_eq!(original, snapshot, "the upload is never modified");
        assert_eq!(patched.len(), SAVE_LEN);
        for (i, (a, b)) in original.iter().zip(&patched).enumerate() {
            if !CREDENTIAL_REGION.contains(&i) {
                assert_eq!(a, b, "byte {i:#x} changed");
            }
        }
        // TLS journal pages stay byte-identical.
        assert_eq!(original[0x3F000..0x3F200], patched[0x3F000..0x3F200]);
        assert_eq!(
            patched[PAGE_A..PAGE_A + PAGE_LEN],
            patched[PAGE_B..PAGE_B + PAGE_LEN]
        );
        assert_eq!(read_credential(&patched), Ok(Some(credential.clone())));
        assert_eq!(verify_patched(&original, &patched, &credential), Ok(()));

        let mut outside = patched.clone();
        outside[0x3F000] ^= 1;
        assert!(verify_patched(&original, &outside, &credential).is_err());
        assert!(verify_patched(&original, &patched, &self::credential(2)).is_err());
    }

    #[test]
    fn empty_pages_are_zero_or_ff() {
        let mut save = sample_save();
        assert_eq!(read_credential(&save), Ok(None));
        save[CREDENTIAL_REGION].fill(0);
        assert_eq!(read_credential(&save), Ok(None));
        save[PAGE_A..PAGE_A + PAGE_LEN].fill(0xFF);
        assert_eq!(read_credential(&save), Ok(None));
    }

    #[test]
    fn unknown_or_unsupported_page_data_is_rejected() {
        let base = sample_save();
        let mut garbage = base.clone();
        garbage[PAGE_A + 10] = 1;
        assert_eq!(
            read_credential(&garbage),
            Err(SaveError::UnsupportedCredential)
        );

        let valid = write_credential(&base, &credential(1)).unwrap();
        let mut v2 = valid.clone();
        v2[PAGE_A + 4] = 2;
        assert_eq!(read_credential(&v2), Err(SaveError::UnsupportedCredential));

        // Intact checksum but nonzero flags: a variant this version does not know.
        let mut flagged = encode_page(&credential(1));
        flagged[0x0C] = 1;
        let crc = crc32fast::hash(&flagged[..CRC_OFFSET]);
        flagged[CRC_OFFSET..0x80].copy_from_slice(&crc.to_le_bytes());
        let mut save = valid.clone();
        save[PAGE_B..PAGE_B + PAGE_LEN].copy_from_slice(&flagged);
        assert_eq!(
            read_credential(&save),
            Err(SaveError::UnsupportedCredential)
        );

        // Uppercase IDs are not PocketBase IDs.
        let mut upper = encode_page(&credential(1));
        upper[0x20] = b'A';
        let crc = crc32fast::hash(&upper[..CRC_OFFSET]);
        upper[CRC_OFFSET..0x80].copy_from_slice(&crc.to_le_bytes());
        let mut save = base.clone();
        save[PAGE_A..PAGE_A + PAGE_LEN].copy_from_slice(&upper);
        assert_eq!(
            read_credential(&save),
            Err(SaveError::UnsupportedCredential)
        );
    }

    #[test]
    fn a_damaged_copy_is_recovered_from_its_valid_mirror() {
        let credential = credential(3);
        let valid = write_credential(&sample_save(), &credential).unwrap();
        for page in [PAGE_A, PAGE_B] {
            let mut damaged = valid.clone();
            damaged[page + 0x45] ^= 0x10;
            assert_eq!(read_credential(&damaged), Ok(Some(credential.clone())));
            let mut padding = valid.clone();
            padding[page + 0xF0] = 1;
            assert_eq!(read_credential(&padding), Ok(Some(credential.clone())));
            let mut half_written = valid.clone();
            half_written[page..page + PAGE_LEN].fill(0xFF);
            assert_eq!(read_credential(&half_written), Ok(Some(credential.clone())));
        }
        let mut both = valid.clone();
        both[PAGE_A + 0x45] ^= 0x10;
        both[PAGE_B + 0x45] ^= 0x10;
        assert_eq!(read_credential(&both), Err(SaveError::DamagedCopies));
        let mut one_left_empty = both.clone();
        one_left_empty[PAGE_B..PAGE_B + PAGE_LEN].fill(0);
        assert_eq!(
            read_credential(&one_left_empty),
            Err(SaveError::DamagedCopies)
        );
    }

    fn with_pages(a: &Credential, b: &Credential) -> Vec<u8> {
        let mut save = sample_save();
        save[PAGE_A..PAGE_A + PAGE_LEN].copy_from_slice(&encode_page(a));
        save[PAGE_B..PAGE_B + PAGE_LEN].copy_from_slice(&encode_page(b));
        save
    }

    #[test]
    fn mirrors_pick_the_highest_generation_and_reject_conflicts() {
        let old = Credential {
            login_counter: 9,
            ..credential(4)
        };
        let new = Credential {
            login_counter: 10,
            ..credential(5)
        };
        assert_eq!(
            read_credential(&with_pages(&old, &new)),
            Ok(Some(new.clone()))
        );
        assert_eq!(
            read_credential(&with_pages(&new, &old)),
            Ok(Some(new.clone()))
        );

        let same_generation = Credential {
            login_counter: 11,
            ..credential(5)
        };
        assert_eq!(
            read_credential(&with_pages(&new, &same_generation)),
            Err(SaveError::ConflictingCopies)
        );
        for other in [
            Credential {
                device_id: "zzzzzzzzzzzzzzz".into(),
                ..credential(6)
            },
            Credential {
                player_id: "yyyyyyyyyyyyyyy".into(),
                ..credential(6)
            },
            Credential {
                secret: [7; 32],
                ..credential(6)
            },
        ] {
            assert_eq!(
                read_credential(&with_pages(&new, &other)),
                Err(SaveError::ConflictingCopies)
            );
        }
    }

    #[test]
    fn debug_output_never_contains_the_secret() {
        let text = format!("{:?}", credential(1));
        assert!(text.contains("<redacted>"));
        assert!(!text.contains("secret: ["));
    }
}
