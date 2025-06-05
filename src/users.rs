use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

pub(crate) fn hash_password(pw: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .expect("Failed to hash password")
        .to_string()
}

pub(crate) fn verify_password(hash: &str, pw: &str) -> bool {
    let parsed_hash = PasswordHash::new(hash).expect("Failed to parse hash");
    Argon2::default()
        .verify_password(pw.as_bytes(), &parsed_hash)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::{hash_password, verify_password};

    #[test]
    fn test_hash_password() {
        let hashed = hash_password("secret");
        assert!(verify_password(&hashed, "secret"));
    }
}