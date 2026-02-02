//! Password hashing and verification utilities.
//!
//! Functions in this module provide a thin wrapper around the `argon2` crate
//! to hash and verify user passwords for authentication purposes.

use argon2::{
    Argon2,
    password_hash::{
        Error,
        PasswordHash,
        PasswordHasher,
        PasswordVerifier,
        SaltString,
        rand_core::OsRng,
    },
};

/// Hash a password using the provided Argon2 instance.
///
/// # Errors
/// Returns any error produced by the underlying hashing implementation.
#[must_use = "handle the result"]
pub fn hash_password(argon2: &Argon2, pw: &str) -> Result<String, Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(argon2.hash_password(pw.as_bytes(), &salt)?.to_string())
}

/// Verify a password against a stored Argon2 hash.
///
/// Returns `true` if the password matches the hash, `false` otherwise.
/// Invalid or unparseable hashes yield `false` without panicking.
pub(crate) fn verify_password(hash: &str, pw: &str) -> bool {
    let Ok(parsed_hash) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(pw.as_bytes(), &parsed_hash)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use argon2::Argon2;
    use rstest::{fixture, rstest};

    use super::{hash_password, verify_password};

    #[fixture]
    fn argon2_instance() -> Argon2<'static> {
        let argon2 = Argon2::default();
        let _ = argon2.params().m_cost();
        argon2
    }

    #[rstest]
    #[case("secret", "secret", true)]
    #[case("secret", "not-secret", false)]
    fn test_verify_password_matches_expected(
        argon2_instance: Argon2<'static>,
        #[case] plain: &str,
        #[case] candidate: &str,
        #[case] expected: bool,
    ) {
        let hashed = hash_password(&argon2_instance, plain).expect("hash password");
        assert_eq!(verify_password(&hashed, candidate), expected);
    }

    #[test]
    fn test_verify_password_rejects_invalid_hash() {
        assert!(!verify_password("not-a-hash", "secret"));
    }
}
