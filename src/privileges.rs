//! User access privilege bits from the Hotline protocol.
//!
//! Field 110 (User Access) contains a bitmap representing the privileges
//! granted to a user account. Each bit corresponds to a specific operation
//! that the user may or may not be allowed to perform. See `docs/protocol.md`
//! for the full specification.

use bitflags::bitflags;

bitflags! {
    /// User access privileges from Hotline protocol field 110.
    ///
    /// Each bit corresponds to a specific permission. The bit positions are
    /// defined in `docs/protocol.md` under "Access Privilege Bits (Field 110)".
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct Privileges: u64 {
        /// Bit 0: Delete File - User may delete files.
        const DELETE_FILE = 1 << 0;
        /// Bit 1: Upload File - User may upload files.
        const UPLOAD_FILE = 1 << 1;
        /// Bit 2: Download File - User may download files and view file listings.
        const DOWNLOAD_FILE = 1 << 2;
        /// Bit 3: Rename File - User may rename files.
        const RENAME_FILE = 1 << 3;
        /// Bit 4: Move File - User may move files between folders.
        const MOVE_FILE = 1 << 4;
        /// Bit 5: Create Folder - User may create new folders.
        const CREATE_FOLDER = 1 << 5;
        /// Bit 6: Delete Folder - User may delete folders.
        const DELETE_FOLDER = 1 << 6;
        /// Bit 7: Rename Folder - User may rename folders.
        const RENAME_FOLDER = 1 << 7;
        /// Bit 8: Move Folder - User may move folders.
        const MOVE_FOLDER = 1 << 8;
        /// Bit 9: Read Chat - User may read chat messages.
        const READ_CHAT = 1 << 9;
        /// Bit 10: Send Chat - User may send chat messages.
        const SEND_CHAT = 1 << 10;
        /// Bit 11: Open Chat - User may open/create chat rooms.
        const OPEN_CHAT = 1 << 11;
        /// Bit 12: Close Chat - User may close chat rooms.
        const CLOSE_CHAT = 1 << 12;
        /// Bit 13: Show in List - User appears in the user list.
        const SHOW_IN_LIST = 1 << 13;
        /// Bit 14: Create User - User may create new user accounts.
        const CREATE_USER = 1 << 14;
        /// Bit 15: Delete User - User may delete user accounts.
        const DELETE_USER = 1 << 15;
        /// Bit 16: Open User - User may view user account details.
        const OPEN_USER = 1 << 16;
        /// Bit 17: Modify User - User may modify user accounts.
        const MODIFY_USER = 1 << 17;
        /// Bit 18: Change Own Password - User may change their own password.
        const CHANGE_OWN_PASSWORD = 1 << 18;
        /// Bit 19: Send Private Message - User may send private messages.
        const SEND_PRIVATE_MESSAGE = 1 << 19;
        /// Bit 20: News Read Article - User may read news articles.
        const NEWS_READ_ARTICLE = 1 << 20;
        /// Bit 21: News Post Article - User may post news articles.
        const NEWS_POST_ARTICLE = 1 << 21;
        /// Bit 22: Disconnect User - User may disconnect other users.
        const DISCONNECT_USER = 1 << 22;
        /// Bit 23: Cannot be Disconnected - User cannot be disconnected by others.
        const CANNOT_BE_DISCONNECTED = 1 << 23;
        /// Bit 24: Get Client Info - User may view other users' info.
        const GET_CLIENT_INFO = 1 << 24;
        /// Bit 25: Upload Anywhere - User may upload to any folder.
        const UPLOAD_ANYWHERE = 1 << 25;
        /// Bit 26: Any Name - User may use any display name.
        const ANY_NAME = 1 << 26;
        /// Bit 27: No Agreement - User does not need to accept server agreement.
        const NO_AGREEMENT = 1 << 27;
        /// Bit 28: Set File Comment - User may set file comments.
        const SET_FILE_COMMENT = 1 << 28;
        /// Bit 29: Set Folder Comment - User may set folder comments.
        const SET_FOLDER_COMMENT = 1 << 29;
        /// Bit 30: View Drop Boxes - User may view contents of drop boxes.
        const VIEW_DROP_BOXES = 1 << 30;
        /// Bit 31: Make Alias - User may create file/folder aliases.
        const MAKE_ALIAS = 1 << 31;
        /// Bit 32: Broadcast - User may send broadcast messages.
        const BROADCAST = 1 << 32;
        /// Bit 33: News Delete Article - User may delete news articles.
        const NEWS_DELETE_ARTICLE = 1 << 33;
        /// Bit 34: News Create Category - User may create news categories.
        const NEWS_CREATE_CATEGORY = 1 << 34;
        /// Bit 35: News Delete Category - User may delete news categories.
        const NEWS_DELETE_CATEGORY = 1 << 35;
        /// Bit 36: News Create Folder - User may create news folders/bundles.
        const NEWS_CREATE_FOLDER = 1 << 36;
        /// Bit 37: News Delete Folder - User may delete news folders/bundles.
        const NEWS_DELETE_FOLDER = 1 << 37;
    }
}

impl Privileges {
    /// Default privileges for a newly authenticated user.
    ///
    /// Grants basic read/download access and communication privileges without
    /// administrative capabilities. This matches typical regular user
    /// permissions. TODO(task 5.1): Load privileges from user account in
    /// database rather than using this default.
    #[must_use]
    pub const fn default_user() -> Self {
        Self::from_bits_truncate(
            Self::DOWNLOAD_FILE.bits()
                | Self::READ_CHAT.bits()
                | Self::SEND_CHAT.bits()
                | Self::SHOW_IN_LIST.bits()
                | Self::SEND_PRIVATE_MESSAGE.bits()
                | Self::NEWS_READ_ARTICLE.bits()
                | Self::NEWS_POST_ARTICLE.bits()
                | Self::GET_CLIENT_INFO.bits()
                | Self::CHANGE_OWN_PASSWORD.bits(),
        )
    }

    /// Full administrative privileges.
    ///
    /// Grants all available permissions. Use with caution.
    #[must_use]
    pub const fn admin() -> Self { Self::all() }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[test]
    fn default_is_empty() {
        let privs = Privileges::default();
        assert!(privs.is_empty());
    }

    #[test]
    fn default_user_has_download() {
        let privs = Privileges::default_user();
        assert!(privs.contains(Privileges::DOWNLOAD_FILE));
    }

    #[test]
    fn default_user_has_read_chat() {
        let privs = Privileges::default_user();
        assert!(privs.contains(Privileges::READ_CHAT));
    }

    #[test]
    fn default_user_has_news_read() {
        let privs = Privileges::default_user();
        assert!(privs.contains(Privileges::NEWS_READ_ARTICLE));
    }

    #[test]
    fn default_user_has_news_post() {
        let privs = Privileges::default_user();
        assert!(privs.contains(Privileges::NEWS_POST_ARTICLE));
    }

    #[test]
    fn default_user_lacks_admin_privs() {
        let privs = Privileges::default_user();
        assert!(!privs.contains(Privileges::CREATE_USER));
        assert!(!privs.contains(Privileges::DELETE_USER));
        assert!(!privs.contains(Privileges::DISCONNECT_USER));
        assert!(!privs.contains(Privileges::BROADCAST));
    }

    #[test]
    fn admin_has_all_privileges() {
        let privs = Privileges::admin();
        assert!(privs.contains(Privileges::DELETE_FILE));
        assert!(privs.contains(Privileges::CREATE_USER));
        assert!(privs.contains(Privileges::NEWS_DELETE_FOLDER));
    }

    #[rstest]
    #[case(Privileges::DELETE_FILE, 0)]
    #[case(Privileges::UPLOAD_FILE, 1)]
    #[case(Privileges::DOWNLOAD_FILE, 2)]
    #[case(Privileges::RENAME_FILE, 3)]
    #[case(Privileges::MOVE_FILE, 4)]
    #[case(Privileges::CREATE_FOLDER, 5)]
    #[case(Privileges::DELETE_FOLDER, 6)]
    #[case(Privileges::RENAME_FOLDER, 7)]
    #[case(Privileges::MOVE_FOLDER, 8)]
    #[case(Privileges::READ_CHAT, 9)]
    #[case(Privileges::SEND_CHAT, 10)]
    #[case(Privileges::OPEN_CHAT, 11)]
    #[case(Privileges::CLOSE_CHAT, 12)]
    #[case(Privileges::SHOW_IN_LIST, 13)]
    #[case(Privileges::CREATE_USER, 14)]
    #[case(Privileges::DELETE_USER, 15)]
    #[case(Privileges::OPEN_USER, 16)]
    #[case(Privileges::MODIFY_USER, 17)]
    #[case(Privileges::CHANGE_OWN_PASSWORD, 18)]
    #[case(Privileges::SEND_PRIVATE_MESSAGE, 19)]
    #[case(Privileges::NEWS_READ_ARTICLE, 20)]
    #[case(Privileges::NEWS_POST_ARTICLE, 21)]
    #[case(Privileges::DISCONNECT_USER, 22)]
    #[case(Privileges::CANNOT_BE_DISCONNECTED, 23)]
    #[case(Privileges::GET_CLIENT_INFO, 24)]
    #[case(Privileges::UPLOAD_ANYWHERE, 25)]
    #[case(Privileges::ANY_NAME, 26)]
    #[case(Privileges::NO_AGREEMENT, 27)]
    #[case(Privileges::SET_FILE_COMMENT, 28)]
    #[case(Privileges::SET_FOLDER_COMMENT, 29)]
    #[case(Privileges::VIEW_DROP_BOXES, 30)]
    #[case(Privileges::MAKE_ALIAS, 31)]
    #[case(Privileges::BROADCAST, 32)]
    #[case(Privileges::NEWS_DELETE_ARTICLE, 33)]
    #[case(Privileges::NEWS_CREATE_CATEGORY, 34)]
    #[case(Privileges::NEWS_DELETE_CATEGORY, 35)]
    #[case(Privileges::NEWS_CREATE_FOLDER, 36)]
    #[case(Privileges::NEWS_DELETE_FOLDER, 37)]
    fn privilege_bit_position(#[case] priv_flag: Privileges, #[case] expected_bit: u32) {
        assert_eq!(
            priv_flag.bits(),
            1u64 << expected_bit,
            "privilege {priv_flag:?} should be at bit {expected_bit}"
        );
    }

    #[test]
    fn privileges_can_be_combined() {
        let combined = Privileges::DOWNLOAD_FILE | Privileges::UPLOAD_FILE;
        assert!(combined.contains(Privileges::DOWNLOAD_FILE));
        assert!(combined.contains(Privileges::UPLOAD_FILE));
        assert!(!combined.contains(Privileges::DELETE_FILE));
    }

    #[test]
    fn privileges_from_bits_truncate() {
        let privs = Privileges::from_bits_truncate(0b111);
        assert!(privs.contains(Privileges::DELETE_FILE));
        assert!(privs.contains(Privileges::UPLOAD_FILE));
        assert!(privs.contains(Privileges::DOWNLOAD_FILE));
        assert!(!privs.contains(Privileges::RENAME_FILE));
    }
}
