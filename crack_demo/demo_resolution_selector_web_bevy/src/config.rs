

    #[cfg(feature = "web")]
    pub const DATA_BASE_URL: &str = "http://192.168.122.237:1973/";
    #[cfg(not(feature = "web"))]
    pub const DATA_BASE_URL: &str = "http://127.0.0.1:1973/";