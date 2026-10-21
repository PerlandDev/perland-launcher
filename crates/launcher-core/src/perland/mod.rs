//! Perland player accounts: the Engine's `/api/v1/player` routes. A player token is a
//! credential: nothing here prints one, and every error names the Engine's host only.
pub mod api;
pub mod loopback;
pub mod panel;
pub mod pkce;
pub mod session;

pub use api::{
    CreateAllowance, CreateFreeServer, Deletion, DiscordAccount, Disk, EmailStarted, FreeLimits,
    FreeServer, FreeServerState, LinkCode, MemoryLimits, MinecraftAccount, PackLimits, Player,
    PlayerClient, PlayerConfig, PlayerStats, Relation, ServerStats, SignInMethod, StatsInvite,
    TokenAnswer, YourServer,
};
pub use loopback::{Callback, Language, Loopback};
pub use panel::{manage_url, panel_url};
pub use pkce::{authorize_url, Pkce};
pub use session::{
    KeyringStore, MemoryStore, Profile, SecretError, SecretStore, Session, SessionStore,
};
