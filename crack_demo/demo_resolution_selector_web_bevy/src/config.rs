#[cfg(feature = "web")]
pub const DATA_BASE_URL: &str = "https://pantelimon.alt-f4.ro/";
#[cfg(not(feature = "web"))]
pub const DATA_BASE_URL: &str = "http://127.0.0.1:1973/";
// pub const DATA_BASE_URL: &str = "https://pantelimon.alt-f4.ro/";
