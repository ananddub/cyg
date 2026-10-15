//! Field validation, in the order with the wording. A client SDK's
//! retry logic branches on these messages, so they are transcribed rather than
//! paraphrased, and the byte-length windows are byte lengths because that is
//! what Go's `len()` measures.

use rand::Rng;

use crate::error::V2Error;

/// Go's `[[:cntrl:]]` over bytes: the C0 range plus DEL.
fn is_control(byte: u8) -> bool {
    byte < 0x20 || byte == 0x7f
}

/// `invalidCharsRegex` — control characters and ASCII whitespace; a device id with a
/// space in it is refused.
pub(crate) fn has_space_or_control(raw: &str) -> bool {
    raw.bytes().any(|b| b == b' ' || is_control(b))
}

/// `invalidUsernameRegex`: allows spaces (client API, not the console's rule) and
/// refuses `[]` — a transcription quirk kept so both servers refuse the same names.
fn has_bad_username_chars(raw: &str) -> bool {
    raw.bytes().any(is_control) || raw.contains("[]")
}

/// The `username` query parameter of every `Authenticate*`. Absent means BaaS
/// generates one, so an empty string is a request, not an error.
pub fn username(raw: &str) -> Result<String, V2Error> {
    if raw.is_empty() {
        return Ok(generate());
    }
    if has_bad_username_chars(raw) {
        return Err(V2Error::invalid_argument(
            "Username invalid, no spaces or control characters allowed.",
        ));
    }
    if raw.len() > 128 {
        return Err(V2Error::invalid_argument(
            "Username invalid, must be 1-128 bytes.",
        ));
    }
    Ok(raw.to_owned())
}

/// `generateUsername`: ten letters from the exact alphabet, so accounts created
/// here look like accounts created there.
fn generate() -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut rng = rand::thread_rng();
    (0..10)
        .map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char)
        .collect()
}

/// A provider id checked the way `validateDeviceID`/`validateCustomID` do: required,
/// then character class, then byte window. `label` names it in the message.
pub fn id_field(raw: &str, label: &str, min: usize) -> Result<(), V2Error> {
    if raw.is_empty() {
        return Err(V2Error::invalid_argument(format!("{label} is required.")));
    }
    if has_space_or_control(raw) {
        return Err(V2Error::invalid_argument(format!(
            "{label} invalid, no spaces or control characters allowed."
        )));
    }
    if raw.len() < min || raw.len() > 128 {
        return Err(V2Error::invalid_argument(format!(
            "{label} invalid, must be {min}-128 bytes."
        )));
    }
    Ok(())
}

/// Which credential login the email endpoint is asking for. BaaS picks purely on
/// whether an address was sent; a username login can never create an account.
#[derive(Debug)]
pub enum Login {
    Email {
        email: String,
        password: String,
        username: String,
    },
    Username {
        username: String,
        password: String,
    },
}

/// The email body's checks in the gateway's order: address shape before password
/// length, both before the username (required only for a username login).
pub fn email_login(email: &str, password: &str, requested: &str) -> Result<Login, V2Error> {
    let username_login = email.is_empty();
    if !username_login {
        if has_space_or_control(email) {
            return Err(V2Error::invalid_argument(
                "Invalid email address, no spaces or control characters allowed.",
            ));
        }
        if !has_email_shape(email) {
            return Err(V2Error::invalid_argument("Invalid email address format."));
        }
        if email.len() < 10 || email.len() > 255 {
            return Err(V2Error::invalid_argument(
                "Invalid email address, must be 10-255 bytes.",
            ));
        }
    }
    if password.len() < 8 {
        return Err(V2Error::invalid_argument(
            "Password must be at least 8 characters long.",
        ));
    }

    if username_login && requested.is_empty() {
        return Err(V2Error::invalid_argument(
            "Username is required when email address is not supplied.",
        ));
    }
    let username = username(requested)?;
    Ok(if username_login {
        Login::Username {
            username,
            password: password.to_owned(),
        }
    } else {
        Login::Email {
            email: email.to_lowercase(),
            password: password.to_owned(),
            username,
        }
    })
}

/// Go's `^.+@.+\..+$`: every `@` is a candidate split, not only the first, because the
/// pattern asks that *some* prefix and *some* middle exist.
pub(crate) fn has_email_shape(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    (1..bytes.len()).any(|at| {
        bytes[at] == b'@'
            && bytes[at + 1..].len() >= 3
            && bytes[at + 1..bytes.len() - 1].contains(&b'.')
    })
}
