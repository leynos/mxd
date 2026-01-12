//! Connection-level user preference flags.
//!
//! These flags represent user preferences sent during login (field 113, Options)
//! that control how the user interacts with other users on the server. They are
//! stored per-session and can be updated via SetClientUserInfo (transaction 304).

use bitflags::bitflags;

bitflags! {
    /// User preference flags from Hotline protocol field 113 (Options).
    ///
    /// These flags control how the user receives messages and chat invitations.
    /// They are set during the Agreed transaction (121) and can be updated via
    /// SetClientUserInfo (304).
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct ConnectionFlags: u8 {
        /// Bit 0: Refuse private messages from other users.
        const REFUSE_PRIVATE_MESSAGES = 1 << 0;
        /// Bit 1: Refuse private chat invitations.
        const REFUSE_CHAT_INVITES = 1 << 1;
        /// Bit 2: Automatic response enabled.
        ///
        /// When set, the server will automatically send the user's
        /// auto-response text (field 215) to users who send private messages.
        const AUTOMATIC_RESPONSE = 1 << 2;
    }
}

impl ConnectionFlags {
    /// Check if the user is refusing private messages.
    #[must_use]
    pub const fn refuses_messages(self) -> bool {
        self.contains(Self::REFUSE_PRIVATE_MESSAGES)
    }

    /// Check if the user is refusing chat invitations.
    #[must_use]
    pub const fn refuses_chat(self) -> bool {
        self.contains(Self::REFUSE_CHAT_INVITES)
    }

    /// Check if automatic response is enabled.
    #[must_use]
    pub const fn has_auto_response(self) -> bool {
        self.contains(Self::AUTOMATIC_RESPONSE)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[test]
    fn default_is_empty() {
        let flags = ConnectionFlags::default();
        assert!(flags.is_empty());
        assert!(!flags.refuses_messages());
        assert!(!flags.refuses_chat());
        assert!(!flags.has_auto_response());
    }

    #[rstest]
    #[case(ConnectionFlags::REFUSE_PRIVATE_MESSAGES, 0)]
    #[case(ConnectionFlags::REFUSE_CHAT_INVITES, 1)]
    #[case(ConnectionFlags::AUTOMATIC_RESPONSE, 2)]
    fn flag_bit_position(#[case] flag: ConnectionFlags, #[case] expected_bit: u32) {
        assert_eq!(
            flag.bits(),
            1u8 << expected_bit,
            "flag {:?} should be at bit {}",
            flag,
            expected_bit
        );
    }

    #[test]
    fn refuses_messages_helper() {
        let flags = ConnectionFlags::REFUSE_PRIVATE_MESSAGES;
        assert!(flags.refuses_messages());
        assert!(!flags.refuses_chat());
    }

    #[test]
    fn refuses_chat_helper() {
        let flags = ConnectionFlags::REFUSE_CHAT_INVITES;
        assert!(!flags.refuses_messages());
        assert!(flags.refuses_chat());
    }

    #[test]
    fn auto_response_helper() {
        let flags = ConnectionFlags::AUTOMATIC_RESPONSE;
        assert!(flags.has_auto_response());
        assert!(!flags.refuses_messages());
    }

    #[test]
    fn combined_flags() {
        let flags =
            ConnectionFlags::REFUSE_PRIVATE_MESSAGES | ConnectionFlags::REFUSE_CHAT_INVITES;
        assert!(flags.refuses_messages());
        assert!(flags.refuses_chat());
        assert!(!flags.has_auto_response());
    }

    #[test]
    fn from_bits_truncate() {
        let flags = ConnectionFlags::from_bits_truncate(0b011);
        assert!(flags.refuses_messages());
        assert!(flags.refuses_chat());
        assert!(!flags.has_auto_response());
    }
}
