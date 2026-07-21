//! Chat subsystem for net_crackpipe.
//!
//! Provides global chat, direct messaging, presence tracking, chat tickets,
//! and raw gossip rooms.

/// Chat constants (topic IDs, intervals, timeouts).
pub mod chat_const;

/// Chat controller: message dispatch, sender/receiver, room management.
pub mod chat_controller;

/// Presence tracking: flags, lists, updates, and notifications.
pub mod chat_presence;

/// Chat tickets for joining rooms with bootstrap nodes.
pub mod chat_ticket;

/// Direct message protocol between two nodes.
pub mod direct_message;

/// Global chat room type, messages, presence, and bootstrap queries.
pub mod global_chat;

/// Raw gossip-based chat room implementation.
pub mod room_raw;
