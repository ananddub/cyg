use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};

use crate::error::V2Error;

pub fn hash_password(password: &str) -> Result<Vec<u8>, V2Error> {
    let salt = SaltString::generate(&mut OsRng);
    let phc = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| V2Error::internal(e.to_string()))?;
    Ok(phc.to_string().into_bytes())
}

pub fn verify_password(password: &str, stored: &[u8]) -> bool {
    let Ok(stored_str) = std::str::from_utf8(stored) else {
        return false;
    };
    let Ok(parsed) = PasswordHash::new(stored_str) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}
