//! Password storage. Argon2id rather than the bcrypt. What is stored is
//! the full PHC string (`$argon2id$v=19$m=…$salt$hash` as UTF-8), so the
//! parameters travel with the hash and a future change still verifies old
//! accounts. `users.password` is bytea with a 32000-byte cap, far above the
//! ~120 bytes written here.

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};

use crate::error::V2Error;

/// Hash a password and return the PHC string bytes to store.
pub fn hash_password(password: &str) -> Result<Vec<u8>, V2Error> {
    let salt = SaltString::generate(&mut OsRng);
    let phc = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| V2Error::internal(e.to_string()))?;
    Ok(phc.to_string().into_bytes())
}

/// Constant-time by construction: Argon2's verifier compares the recomputed hash.
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
