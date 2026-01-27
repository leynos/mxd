//! Privilege bit constants for the session gating model.
//!
//! These constants mirror the values in `src/privileges.rs` and must be kept
//! in sync manually. Any drift indicates a synchronization failure that must
//! be corrected before verification results can be trusted.
//!
//! The constants are defined as raw `u64` values rather than using bitflags
//! to keep this crate dependency-light.

/// Bit 2: Download File - User may download files and view file listings.
pub const DOWNLOAD_FILE: u64 = 1 << 2;

/// Bit 9: Read Chat - User may read chat messages.
pub const READ_CHAT: u64 = 1 << 9;

/// Bit 10: Send Chat - User may send chat messages.
pub const SEND_CHAT: u64 = 1 << 10;

/// Bit 13: Show in List - User appears in the user list.
pub const SHOW_IN_LIST: u64 = 1 << 13;

/// Bit 18: Change Own Password - User may change their own password.
pub const CHANGE_OWN_PASSWORD: u64 = 1 << 18;

/// Bit 19: Send Private Message - User may send private messages.
pub const SEND_PRIVATE_MESSAGE: u64 = 1 << 19;

/// Bit 20: News Read Article - User may read news articles.
pub const NEWS_READ_ARTICLE: u64 = 1 << 20;

/// Bit 21: News Post Article - User may post news articles.
pub const NEWS_POST_ARTICLE: u64 = 1 << 21;

/// Bit 24: Get Client Info - User may view other users' info.
pub const GET_CLIENT_INFO: u64 = 1 << 24;

/// Bit 14: Create User - User may create new user accounts (admin privilege).
pub const CREATE_USER: u64 = 1 << 14;

/// Bit 22: Disconnect User - User may disconnect other users (admin privilege).
pub const DISCONNECT_USER: u64 = 1 << 22;

/// Default privileges for a standard authenticated user.
///
/// This composite matches `Privileges::default_user()` in `src/privileges.rs`.
pub const DEFAULT_USER_PRIVILEGES: u64 = DOWNLOAD_FILE
    | READ_CHAT
    | SEND_CHAT
    | SHOW_IN_LIST
    | SEND_PRIVATE_MESSAGE
    | NEWS_READ_ARTICLE
    | NEWS_POST_ARTICLE
    | GET_CLIENT_INFO
    | CHANGE_OWN_PASSWORD;

/// Empty privilege set representing no permissions.
pub const NO_PRIVILEGES: u64 = 0;

/// Full administrative privileges (all bits set up to bit 37).
pub const ADMIN_PRIVILEGES: u64 = (1 << 38) - 1;

/// Checks whether a privilege set contains the required privilege.
#[must_use]
pub const fn has_privilege(privileges: u64, required: u64) -> bool {
    (privileges & required) == required
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(DOWNLOAD_FILE, 1 << 2, "DOWNLOAD_FILE should be at bit 2")]
    #[case(
        NEWS_READ_ARTICLE,
        1 << 20,
        "NEWS_READ_ARTICLE should be at bit 20"
    )]
    #[case(
        NEWS_POST_ARTICLE,
        1 << 21,
        "NEWS_POST_ARTICLE should be at bit 21"
    )]
    fn privilege_bit_positions(#[case] actual: u64, #[case] expected: u64, #[case] message: &str) {
        assert_eq!(actual, expected, "{message}");
    }

    #[rstest]
    #[case(
        DEFAULT_USER_PRIVILEGES,
        DOWNLOAD_FILE,
        true,
        "default user has download"
    )]
    #[case(
        DEFAULT_USER_PRIVILEGES,
        NEWS_READ_ARTICLE,
        true,
        "default user has news read"
    )]
    #[case(
        DEFAULT_USER_PRIVILEGES,
        NEWS_POST_ARTICLE,
        true,
        "default user has news post"
    )]
    #[case(
        DEFAULT_USER_PRIVILEGES,
        CREATE_USER,
        false,
        "default user lacks create user"
    )]
    #[case(
        DEFAULT_USER_PRIVILEGES,
        DISCONNECT_USER,
        false,
        "default user lacks disconnect user"
    )]
    #[case(NO_PRIVILEGES, DOWNLOAD_FILE, false, "no privileges lacks download")]
    #[case(
        NO_PRIVILEGES,
        NEWS_READ_ARTICLE,
        false,
        "no privileges lacks news read"
    )]
    #[case(ADMIN_PRIVILEGES, DOWNLOAD_FILE, true, "admin has download")]
    #[case(ADMIN_PRIVILEGES, CREATE_USER, true, "admin has create user")]
    #[case(ADMIN_PRIVILEGES, DISCONNECT_USER, true, "admin has disconnect user")]
    fn privilege_membership_cases(
        #[case] privileges: u64,
        #[case] required: u64,
        #[case] expected: bool,
        #[case] message: &str,
    ) {
        assert_eq!(has_privilege(privileges, required), expected, "{message}");
    }
}
