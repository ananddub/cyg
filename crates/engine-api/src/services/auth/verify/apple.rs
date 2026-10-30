//! Sign in with Apple: the identity token is verified against Apple's JWKS
//! and must carry our client id as its audience.

use engine_config::SocialConfig;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use serde_json::Value;

use super::Profile;
use crate::error::V2Error;
use crate::utils::vendor;

const ISSUER: &str = "https://appleid.apple.com";

#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
}

pub(super) async fn verify(config: &SocialConfig, token: &str) -> Result<Profile, V2Error> {
    let response = super::client()
        .get(&config.apple_keys_url)
        .send()
        .await
        .map_err(|err| V2Error::unavailable(format!("Apple key fetch failed: {err}")))?;
    if !response.status().is_success() {
        return Err(V2Error::unavailable(format!(
            "Apple key fetch answered {}.",
            response.status()
        )));
    }
    let jwks: Value = response
        .json()
        .await
        .map_err(|err| V2Error::unavailable(format!("Apple returned an unreadable JWKS: {err}")))?;
    let claims = validate(token, &jwks, &config.apple_client_id)?;
    Ok(Profile {
        id: claims.sub,
        display_name: None,
        avatar_url: None,
    })
}

fn validate(token: &str, jwks: &Value, audience: &str) -> Result<Claims, V2Error> {
    let header = jsonwebtoken::decode_header(token).map_err(|_| vendor::profile("Apple"))?;
    if header.alg != Algorithm::RS256 {
        return Err(vendor::profile("Apple"));
    }
    let kid = header.kid.ok_or_else(|| vendor::profile("Apple"))?;
    let key = decoding_key(jwks, &kid)?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[audience]);
    validation.set_issuer(&[ISSUER]);
    let data = jsonwebtoken::decode::<Claims>(token, &key, &validation)
        .map_err(|_| vendor::profile("Apple"))?;
    if data.claims.sub.is_empty() {
        return Err(vendor::profile("Apple"));
    }
    Ok(data.claims)
}

fn decoding_key(jwks: &Value, kid: &str) -> Result<DecodingKey, V2Error> {
    let keys = jwks
        .get("keys")
        .and_then(Value::as_array)
        .ok_or_else(|| vendor::profile("Apple"))?;
    for key in keys {
        if key.get("kid").and_then(Value::as_str) != Some(kid) {
            continue;
        }
        let modulus = key
            .get("n")
            .and_then(Value::as_str)
            .ok_or_else(|| vendor::profile("Apple"))?;
        let exponent = key
            .get("e")
            .and_then(Value::as_str)
            .ok_or_else(|| vendor::profile("Apple"))?;
        return DecodingKey::from_rsa_components(modulus, exponent)
            .map_err(|_| vendor::profile("Apple"));
    }
    Err(vendor::profile("Apple"))
}

#[cfg(test)]
mod tests {
    use serde::Serialize;
    use serde_json::json;

    use super::*;

    /// A throwaway RSA key; only these tests ever use it.
    const TEST_KEY: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDkvzZqjas8/QCK
hQNVxWoHyw4zeiWPYiFthKjJCbVvgTWqMuY90ZwaOC1cFOVesVHwMwcn396dZF4l
PthJJHmq+QEmeDdKmIUWDrcwgE7E+dJ8UClJYkSrsahqqchfcLBu1tbF0OWj88Nl
4QZrIrGif4SzH9LOj2QJq8r1wsYThBlWzK+t4LUyo24KpZ7cvEDw4C1jA/MAC9C5
KufzacP6LnoZoPIlbAeKqtc9pdoTMY4dOUQPSVnWMDS2DsBGP/GRi8lNMcl7aas3
/ZQSvs3/tzPh5cS0nO9QbZ/OvX7xaeVPtwoR8HMgWDmwHCWMj6lSV1ZjTQJ2lV7J
dSoBgeDHAgMBAAECggEABi7i/itZKxtdpeDlj6536MEE38BX/pG0/Jxe4ZxJErwH
4tNgZOBjG1RUaY5JCu+5GsiOU3YaMt/xKYmS2BtLzKLRdKRAPlx0EH4SVpXJS0rD
dUfHbevhllCFkGiJeYb2xfA5uvwkEPP4Xqsf/MphydYvlJXyIVI/7oD9yfnbV9Cn
s30X0QbpAMYsro6eJCOIO0/DWB34IcSUmuP/vr+UA2o9SrL9f9SH8AMYkkE4fiT5
TyZyW5qmL3o+HL78PJNCZCh1XBqzzmkOcEcJQTE3FMJ8H6PPe/bXdfbNfmQrxF2P
Rl3eU6zZR6hBk+wN4QO11BPpUztOOcO6F7oJnmM7aQKBgQD+QcAvW3a+IweRDcQJ
yY7Nc0jNfJgM7zVjwzr/Mt0Q71Ru9SEOZtzXOlv1baJ4Y5VigdJGKCvJyPRR4Yy6
DyIENzEoTrtuUiYZSYnQWbM/aQD+fjyvhu0V7XrVeUgUc/NQYBW1AZ1bDEUg0YFi
QtaLH02TmvWnH3YrPlFgGT6YLwKBgQDmULBncuVfXutZqq8MHUSp69p6JWH2uPHU
JS1Oc5+XqJ7WMDyE5jkYGhynFzbvnVWVcWKVRR5Qz2TXfNkqn4aztIzX+JnGJ582
HoFqhueIz9sl1h9GMzQWQODOECRlI1BWqaqgDu0mBdXKcmtkaDOvdn1iFaKnIDb+
cVNtW/cC6QKBgHA+jp1Zf35I8Jo0vYqklIpWjplIm8d6AtPb6DupNLa5MUkaplE5
T90kQlyC3erP667lKPMX4Nzf/hv3uqyY6ad6xhTJTS1SzsbzFAgK0Xr43Bt9l0TW
B+xaPpPnSs7csVmWUX7rC5CaRhpK4xeB8jzQwOqcyj3RQdWtWTdXba65AoGAVv0o
85mo2rGuMKrP/MJdOdS0KyhNbPODmz4Dc7smDxEIFRwY75VJATpzkgSpoi0Jsy0l
L/3wKlN64gzstTWVmfBatgPWiMpUTwuMk+CqOLlRK2RJYalbRgBxKENei7if0mu4
30Vj+tV/XzyCJWVaCC63wsErMvZu7wF8ojR3AmkCgYEAw5HwRR52NEXl3VNJny/I
m8X/4bVLfJe9fwErCgtcTS7Y+TusKvpY5NnUWLx4gBrpQJBP9qTbTpz89juLYxyR
bkul1eIejMmcL7sQRdaUosDdZsvvkqNdSyUrPyoow1cK2YWTjWl+JelfR0tjhVME
ATQbviS9erlYt999CoEfVNY=
-----END PRIVATE KEY-----
"#;

    const TEST_N: &str = "5L82ao2rPP0AioUDVcVqB8sOM3olj2IhbYSoyQm1b4E1qjLmPdGcGjgtXBTlXrFR8DMHJ9_enWReJT7YSSR5qvkBJng3SpiFFg63MIBOxPnSfFApSWJEq7GoaqnIX3CwbtbWxdDlo_PDZeEGayKxon-Esx_Szo9kCavK9cLGE4QZVsyvreC1MqNuCqWe3LxA8OAtYwPzAAvQuSrn82nD-i56GaDyJWwHiqrXPaXaEzGOHTlED0lZ1jA0tg7ARj_xkYvJTTHJe2mrN_2UEr7N_7cz4eXEtJzvUG2fzr1-8WnlT7cKEfBzIFg5sBwljI-pUldWY00CdpVeyXUqAYHgxw";

    #[derive(Serialize)]
    struct TestClaims<'a> {
        sub: &'a str,
        aud: &'a str,
        iss: &'a str,
        exp: i64,
    }

    fn jwks() -> Value {
        json!({
            "keys": [{
                "kty": "RSA",
                "kid": "test-key",
                "alg": "RS256",
                "use": "sig",
                "n": TEST_N,
                "e": "AQAB",
            }],
        })
    }

    fn signed(audience: &str, exp: i64) -> String {
        let mut header = jsonwebtoken::Header::new(Algorithm::RS256);
        header.kid = Some("test-key".into());
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(TEST_KEY.as_bytes()).expect("test key");
        jsonwebtoken::encode(
            &header,
            &TestClaims {
                sub: "apple-user-1",
                aud: audience,
                iss: ISSUER,
                exp,
            },
            &key,
        )
        .expect("sign")
    }

    #[test]
    fn a_valid_identity_token_resolves_the_apple_subject() {
        let token = signed("com.example.game", chrono::Utc::now().timestamp() + 600);
        let claims = validate(&token, &jwks(), "com.example.game").expect("valid");
        assert_eq!(claims.sub, "apple-user-1");
    }

    #[test]
    fn a_token_for_another_audience_is_refused() {
        let token = signed("com.evil", chrono::Utc::now().timestamp() + 600);
        assert!(validate(&token, &jwks(), "com.example.game").is_err());
    }

    #[test]
    fn an_expired_identity_token_is_refused() {
        let token = signed("com.example.game", chrono::Utc::now().timestamp() - 3600);
        assert!(validate(&token, &jwks(), "com.example.game").is_err());
    }

    #[test]
    fn an_unknown_key_id_is_refused() {
        let token = signed("com.example.game", chrono::Utc::now().timestamp() + 600);
        let other = json!({ "keys": [{ "kid": "other", "n": TEST_N, "e": "AQAB" }] });
        assert!(validate(&token, &other, "com.example.game").is_err());
    }
}
