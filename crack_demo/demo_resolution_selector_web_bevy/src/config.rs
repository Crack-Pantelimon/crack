//! Remote asset and API base URL for native vs web builds.

#[cfg(feature = "web")]
/// Base URL for map data and assets (HTTPS on web).
pub const DATA_BASE_URL: &str = "https://pantelimon.alt-f4.ro/";
#[cfg(not(feature = "web"))]
/// Base URL for map data and assets (local dev server on native).
pub const DATA_BASE_URL: &str = "http://127.0.0.1:1973/";
// pub const DATA_BASE_URL: &str = "https://pantelimon.alt-f4.ro/";
