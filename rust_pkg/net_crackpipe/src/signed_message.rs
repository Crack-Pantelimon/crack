use std::sync::Arc;

use crate::{
    datetime_now,
    user_identity::{NodeIdentity, UserIdentitySecrets},
};
use anyhow::Result;
use chrono::{DateTime, Utc};
use iroh::{PublicKey, SecretKey};
use iroh_base::Signature;
use serde::{Deserialize, Serialize};

/// Marker trait for types that can be used in signed messages.
/// Requires serde serialization, cloning, debug, equality, send, sync and 'static.
pub trait AcceptableType:
    serde::Serialize
    + for<'a> serde::Deserialize<'a>
    + Clone
    + std::fmt::Debug
    + PartialEq
    + Send
    + Sync
    + 'static
{
}
impl<T> AcceptableType for T where
    T: serde::Serialize
        + for<'a> serde::Deserialize<'a>
        + Clone
        + std::fmt::Debug
        + PartialEq
        + Send
        + Sync
        + 'static
{
}

/// A signed message containing node/user public keys, data, and signatures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedMessage {
    node_pubkey: PublicKey,
    user_pubkey: PublicKey,
    data: Vec<u8>,
    node_signature: Signature,
    user_signature: Signature,
}

impl SignedMessage {
    /// Verifies and decodes a signed message from bytes.
    pub fn verify_and_decode<T: AcceptableType>(bytes: &[u8]) -> Result<WireMessage<T>> {
        let signed_message: Self = postcard::from_bytes(bytes)?;
        let message: WireMessage<T> = postcard::from_bytes(&signed_message.data)?;
        // let signed_message: Self = bincode::deserialize(bytes)?;
        // let message: WireMessage<T> =
        // bincode::deserialize(&signed_message.data)?;

        if message.from.user_id() != &signed_message.user_pubkey {
            return Err(anyhow::anyhow!("user id mismatch"));
        }
        if message.from.node_id() != &signed_message.node_pubkey {
            return Err(anyhow::anyhow!("node id mismatch"));
        }

        signed_message
            .node_pubkey
            .verify(&signed_message.data, &signed_message.node_signature)?;
        signed_message
            .user_pubkey
            .verify(&signed_message.data, &signed_message.user_signature)?;

        Ok(message)
    }
}

/// Signer that holds node/user secrets and creates signed messages.
#[derive(Debug, Clone)]
pub struct MessageSigner {
    pub(crate) node_secret_key: Arc<SecretKey>,
    pub(crate) user_secrets: Arc<UserIdentitySecrets>,
    pub(crate) node_identity: Arc<NodeIdentity>,
}

impl MessageSigner {
    /// Signs and encodes a message, returning the wire bytes and a preview.
    pub fn sign_and_encode<T: AcceptableType>(
        &self,
        message: T,
    ) -> Result<(Vec<u8>, WireMessage<T>)> {
        let timestamp = datetime_now();
        let wire_message = WireMessage {
            _timestamp: timestamp,
            message: message.clone(),
            from: *self.node_identity,
            _message_id: uuid::Uuid::new_v4(),
        };
        let data = postcard::to_stdvec(&wire_message)?;
        // info!("WireMessage size: {:?}", data.len());
        // let compressed = deflate::deflate_bytes_conf(&data, deflate::Compression::Best);
        // info!("Compressed WireMessage size: {:?}", compressed.len());
        // let data = bincode::serialize(&wire_message)?;
        let node_signature = self.node_secret_key.sign(&data);
        let user_signature = self.user_secrets.secret_key().sign(&data);
        let signed_message = SignedMessage {
            node_pubkey: *self.node_identity.node_id(),
            user_pubkey: *self.node_identity.user_id(),
            data,
            node_signature,
            user_signature,
        };
        let encoded = postcard::to_stdvec(&signed_message)?;
        // info!("SignedMessage size: {:?}", encoded.len());
        // let compressed = deflate::deflate_bytes_conf(&encoded, deflate::Compression::Best);
        // info!("Compressed SignedMessage size: {:?}", compressed.len());
        // let encoded = bincode::serialize(&signed_message)?;
        Ok((encoded, wire_message))
    }
}

/// A wire-format message carrying a timestamp, message ID, sender identity, and payload.
/// Serialized and signed as part of [`SignedMessage`].
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct WireMessage<T> {
    /// UTC timestamp when the message was signed.
    pub _timestamp: DateTime<Utc>,
    /// Unique identifier for this wire message.
    pub _message_id: uuid::Uuid,
    /// Identity of the sending node.
    pub from: NodeIdentity,
    /// Signed payload for the chat room type.
    pub message: T,
}

/// A received chat message with sender/receiver timestamps and decoded payload.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ReceivedMessage<T: IChatRoomType> {
    /// Timestamp from the wire message at send time.
    pub _sender_timestamp: DateTime<Utc>,
    /// Local time when the message was received.
    pub _received_timestamp: DateTime<Utc>,
    /// Unique identifier from the wire message.
    pub _message_id: uuid::Uuid,
    /// Identity of the sending node.
    pub from: NodeIdentity,
    /// Decoded message payload for the room type.
    pub message: T::M,
}

/// Chat room type describing message and presence payloads for signed gossip traffic.
pub trait IChatRoomType: AcceptableType {
    /// Message payload type for this room.
    type M: AcceptableType;
    /// Presence payload type for this room.
    type P: AcceptableType;
    /// Returns the default presence value for new joiners.
    fn default_presence() -> Self::P;
}
/// Chat gossip envelope carrying either a presence update or a text message.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ChatMessage<T: IChatRoomType> {
    /// Presence update for this room.
    Presence {
        /// Room-specific presence payload.
        presence: T::P,
    },
    /// Text message for this room.
    Message {
        /// Decoded message text for this room.
        text: T::M,
    },
}
