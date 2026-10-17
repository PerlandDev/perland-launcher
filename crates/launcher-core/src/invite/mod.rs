//! Perland server invites (join-by-link spec §2.1): the link, the Engine's public view of it,
//! and the client that reads that view. Invite tokens are credentials: nothing here prints one.
pub mod client;
pub mod link;
pub mod view;

pub use client::{
    check_engine_url, invite_route, EngineView, InviteClient, InviteLimits, InviteTiming,
};
pub use link::{invite_link_cases, shown_invite, InviteLink, InviteLinkCase, TOKEN_LEN};
pub use view::{
    non_empty, InviteAddress, InviteGame, InviteLauncher, InviteModrinth, InvitePack,
    InvitePlayers, InviteTeam, InviteView, PACK_PENDING, PACK_READY, PACK_UNAVAILABLE,
};

/// Engine and invite URLs, re-exported so the app can name them without depending on reqwest.
pub type Url = reqwest::Url;
