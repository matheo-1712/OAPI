// ──────────────────────────────────────────────
//  Discord Image
// ──────────────────────────────────────────────
pub const LABEL_MEMBER_SINCE: &str = "MEMBRE DEPUIS LE";
pub const LABEL_MESSAGES: &str = "MESSAGES ENVOYÉS";
pub const LABEL_VOCAL_TIME: &str = "TEMPS VOCAL TOTAL";
pub const LABEL_COMPANION: &str = "MEILLEUR AMI LOUTRE";
pub const LABEL_TEXT_CHANNEL: &str = "SALON TEXTUEL PRÉFÉRÉ";
pub const LABEL_VOICE_CHANNEL: &str = "SALON VOCAL PRÉFÉRÉ";
pub const LABEL_ROLES_TITLE: &str = "RÔLES LOUTRES";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_defined() {
        assert_eq!(LABEL_MEMBER_SINCE, "MEMBRE DEPUIS LE");
        assert_eq!(LABEL_ROLES_TITLE, "RÔLES LOUTRES");
    }
}

