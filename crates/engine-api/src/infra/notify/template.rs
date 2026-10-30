//! The words a notification arrives as, kept in one table instead of one
//! function per kind. A template takes the sentence the control plane already
//! wrote (`{{text}}`) and, when there is one to give, the link that stops the
//! channel — so changing what a friend request sounds like in an inbox is a
//! line edit rather than a release.

/// What an unrecognised kind is mailed as: the sentence, nothing invented about
/// it.
const DEFAULT: &str = "{{text}}\n\n";

/// One entry per kind the control plane knows how to announce. A kind missing
/// here is not a bug, it is an unfinished sentence: it falls back to
/// [`DEFAULT`].
const EMAIL: &[(&str, &str)] = &[
    (
        "dm_request",
        "You have a new message.\n\n{{text}}\n\nAnswer it in the game.\n\n",
    ),
    (
        "friend_request",
        "Someone wants to add you.\n\n{{text}}\n\nAccept or decline in the game.\n\n",
    ),
    (
        "friend_accept",
        "A friend request was answered.\n\n{{text}}\n\n",
    ),
    ("friend_remove", "A friendship ended.\n\n{{text}}\n\n"),
    (
        "group_add",
        "You have been added to a group.\n\n{{text}}\n\n",
    ),
    (
        "group_join_request",
        "Someone wants to join a group you run.\n\n{{text}}\n\nDecide in the game.\n\n",
    ),
    (
        "user_banned",
        "Your account has been restricted.\n\n{{text}}\n\n",
    ),
];

/// The body of one mail, footer last. Unknown kinds are sent the plain sentence
/// rather than dropped: the fact happened and this recipient asked to hear about
/// it.
pub fn email(kind: &str, text: &str, unsubscribe: &str) -> String {
    let template = EMAIL
        .iter()
        .find(|(name, _)| *name == kind)
        .map_or(DEFAULT, |(_, body)| *body);
    let body = template.replace("{{text}}", text);
    if unsubscribe.is_empty() {
        return body;
    }
    format!("{body}--\nStop these mails: {unsubscribe}\n")
}
