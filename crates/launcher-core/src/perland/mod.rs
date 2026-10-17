//! Perland player accounts: the Engine's `/api/v1/player` routes. A player token is a
//! credential: nothing here prints one, and every error names the Engine's host only.
pub mod api;

pub use api::{
    DiscordAccount, EmailStarted, Player, PlayerClient, PlayerConfig, Relation, SignInMethod,
    TokenAnswer, YourServer,
};
