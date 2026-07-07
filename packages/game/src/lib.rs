pub mod timestamp {
    pub fn get_timestamp_now_ms() -> i64 {
        chrono::offset::Utc::now().timestamp_millis()
    }
}

pub mod api {
    pub mod game_match {
        use serde::{Serialize, Deserialize};

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        pub struct GameMatch<User> {
            pub match_id: uuid::Uuid,
            pub seed: GameSeed,
            pub time: i64,
            pub users: Vec<User>,
            pub title: String,
            pub type_: GameMatchType,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
        pub enum GameMatchType {
            _1v1,
            ManVsCar(String),
            _40lines,
            _10v10,
            _4v4,
            Blitz,
        }

        pub type GameSeed = [u8; 32];
    }
}

pub mod tet {
    use serde::{Serialize, Deserialize};
    
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct GameState;

    pub type GameSeed = [u8; 32];
}
