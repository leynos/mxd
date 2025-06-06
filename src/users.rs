use argon2::{
    Argon2,
    password_hash::Error,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};

pub fn hash_password(argon2: &Argon2, pw: &str) -> Result<String, Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(argon2.hash_password(pw.as_bytes(), &salt)?.to_string())
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
    use argon2::Argon2;

    #[test]
    fn test_hash_password() {
        let argon2 = Argon2::default();
        let hashed = hash_password(&argon2, "secret").unwrap();
        assert!(verify_password(&hashed, "secret"));
    }
}
