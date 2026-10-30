//! Third-party identity verification. A provider with no credentials is
//! closed; nothing here trusts a token the client merely presented.

mod apple;
mod facebook;
mod google;
mod steam;

use std::sync::OnceLock;
use std::time::Duration;

use engine_config::SocialConfig;

use crate::error::V2Error;
use crate::utils::iap::hex_id;
use crate::utils::vendor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Apple,
    Facebook,
    Google,
    Steam,
}

impl Kind {
    pub(crate) fn parse(name: &str) -> Option<Kind> {
        match name {
            "apple" => Some(Kind::Apple),
            "facebook" => Some(Kind::Facebook),
            "google" => Some(Kind::Google),
            "steam" => Some(Kind::Steam),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Kind::Apple => "Apple",
            Kind::Facebook => "Facebook",
            Kind::Google => "Google",
            Kind::Steam => "Steam",
        }
    }
}

/// The identity a provider confirmed.
#[derive(Debug, Clone)]
pub(crate) struct Profile {
    pub id: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

pub(crate) async fn profile(
    config: &SocialConfig,
    kind: Kind,
    token: &str,
) -> Result<Profile, V2Error> {
    let result = match kind {
        Kind::Google if config.google_configured() => Some(google::verify(config, token).await),
        Kind::Apple if config.apple_configured() => Some(apple::verify(config, token).await),
        Kind::Facebook if config.facebook_configured() => {
            Some(facebook::verify(config, token).await)
        }
        Kind::Steam if config.steam_configured() => Some(steam::verify(config, token).await),
        _ => None,
    };
    match result {
        Some(profile) => profile,
        None if config.allow_unverified => Ok(unverified(kind, token)),
        None if matches!(kind, Kind::Apple | Kind::Steam) => {
            Err(vendor::unconfigured(kind.label()))
        }
        None => Err(vendor::profile(kind.label())),
    }
}

fn unverified(_kind: Kind, token: &str) -> Profile {
    Profile {
        id: hex_id(token),
        display_name: None,
        avatar_url: None,
    }
}

fn client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_default()
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn an_unconfigured_provider_never_trusts_the_token() {
        let config = SocialConfig::default();
        for (kind, message) in [
            (Kind::Apple, "Apple authentication is not configured."),
            (Kind::Steam, "Steam authentication is not configured."),
            (Kind::Google, "Could not authenticate Google profile."),
            (Kind::Facebook, "Could not authenticate Facebook profile."),
        ] {
            let error = profile(&config, kind, "token").await.expect_err("closed");
            assert_eq!(error.message, message);
        }
    }

    #[tokio::test]
    async fn the_development_escape_hatch_hashes_the_token() {
        let config = SocialConfig {
            allow_unverified: true,
            ..SocialConfig::default()
        };
        let first = profile(&config, Kind::Google, "token")
            .await
            .expect("accepted");
        let again = profile(&config, Kind::Google, "token")
            .await
            .expect("stable");
        let other = profile(&config, Kind::Google, "other")
            .await
            .expect("accepted");
        assert_eq!(first.id, again.id);
        assert_ne!(first.id, other.id);
        assert_eq!(first.id, hex_id("token"));
    }

    #[test]
    fn provider_names_parse_case_sensitively() {
        assert_eq!(Kind::parse("google"), Some(Kind::Google));
        assert_eq!(Kind::parse("Google"), None);
        assert_eq!(Kind::parse("gamecenter"), None);
    }
}
