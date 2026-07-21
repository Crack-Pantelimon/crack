use serde::{Deserialize, Serialize};

use crate::{chat::chat_presence::PresenceList, chat::chat_ticket::ChatTicket, IChatRoomType};

/// Marker type identifying the global chat room for [`IChatRoomType`].

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct GlobalChatRoomType;

impl IChatRoomType for GlobalChatRoomType {
    type M = GlobalChatMessageContent;
    type P = GlobalChatPresence;
    fn default_presence() -> Self::P {
        GlobalChatPresence::default()
    }
}
/// Room-specific presence payload broadcast in global chat.
/// Peers advertise their URL, platform, and optional server role.
#[derive(Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize, Default)]
pub struct GlobalChatPresence {
    /// Public URL or endpoint this peer advertises.
    pub url: String,
    /// Client platform identifier (for example OS or runtime).
    pub platform: String,
    /// Present when this peer acts as a game server in global chat.
    pub is_server: Option<()>,
}

/// Message payload types exchanged in the global chat room.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GlobalChatMessageContent {
    /// Plain-text chat message.
    TextMessage {
        /// Message body.
        text: String,
    },
    // MatchmakingMessage {
    //     msg: MatchmakingMessage,
    // },
    /// Request to spectate a match using a room ticket.
    SpectateMatch {
        /// Ticket for joining the match's gossip topic.
        ticket: ChatTicket,
        /// Match or game mode identifier.
        match_type: String,
    },
    /// Bootstrap-node query or response for server discovery.
    BootstrapQuery(GlobalChatBootstrapQuery),
}

/// Queries and responses exchanged with bootstrap nodes in global chat.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum GlobalChatBootstrapQuery {
    /// Ask a bootstrap node for the current list of game servers.
    PlzSendServerList,
    /// Bootstrap reply carrying known server presence entries.
    ServerList {
        /// Snapshot of peers advertising server presence.
        v: PresenceList<GlobalChatPresence>,
    },
}

/// Direct-message handshake steps for initiating or accepting a match.
#[derive(Clone, Debug, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum MatchHandshakeType {
    /// Outbound request to start a match handshake.
    HandshakeRequest,
    /// Peer accepted the handshake.
    AnswerYes,
    /// Peer declined the handshake.
    AnswerNo,
    /// Keepalive ping with a sequence byte.
    Ping(u8),
}

impl From<String> for GlobalChatMessageContent {
    fn from(value: String) -> Self {
        Self::TextMessage { text: value }
    }
}
