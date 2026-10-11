//! Perland server invites (join-by-link spec §2.1): the link, the Engine's public view of it,
//! and the client that reads that view. Invite tokens are credentials: nothing here prints one.
pub mod link;

pub use link::{invite_link_cases, shown_invite, InviteLink, InviteLinkCase, TOKEN_LEN};

/// Engine and invite URLs, re-exported so the app can name them without depending on reqwest.
pub type Url = reqwest::Url;
