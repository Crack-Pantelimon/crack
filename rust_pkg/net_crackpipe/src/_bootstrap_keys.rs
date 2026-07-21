//! Bootstrap secret keys for the net_crackpipe network.
//!
//! This module contains hardcoded bootstrap secret keys used to establish
//! the initial bootstrap nodes for the net_crackpipe network. These keys
//! are used to derive the bootstrap node identities that clients can connect
//! to for initial network discovery.
//!
//! The keys are hardcoded to ensure all clients connect to the same
//! bootstrap nodes on first connection, enabling network bootstrapping.

/// Hardcoded bootstrap secret keys for the net_crackpipe bootstrap nodes.
///
/// This array contains 5 Ed25519 secret keys (32 bytes each) that correspond
/// to the 5 hardcoded bootstrap nodes. These keys are used to derive the
/// corresponding NodeIds that clients use for initial network bootstrap.
///
/// The keys are hardcoded to ensure all clients bootstrap from the same
/// set of bootstrap nodes. In a production deployment, these would typically
/// be replaced with keys controlled by the network operators.
///
/// # Security
///
/// These are public bootstrap keys - they are not secret in the traditional
/// sense since their corresponding public keys (NodeIds) are publicly known
/// and distributed with the client software. The "secret" naming is a
/// convention indicating they are the private half of the bootstrap node
/// identity keypairs.
///
/// # Example
///
/// ```
/// use net_crackpipe::_bootstrap_keys::BOOTSTRAP_SECRET_KEYS;
///
/// // Get the first bootstrap node's secret key
/// let first_key = &BOOTSTRAP_SECRET_KEYS[0];
/// assert_eq!(first_key.len(), 32);
/// ```
pub const BOOTSTRAP_SECRET_KEYS: [[u8; 32]; 5] = [
    [
        165, 48, 79, 186, 71, 176, 49, 168, 36, 74, 114, 142, 97, 104, 107, 16, 184, 0, 60, 202,
        123, 219, 106, 248, 177, 56, 160, 133, 88, 163, 64, 66,
    ],
    [
        92, 203, 25, 13, 168, 94, 65, 89, 32, 183, 156, 237, 101, 97, 252, 62, 29, 190, 140, 35,
        106, 11, 86, 43, 166, 8, 250, 253, 5, 242, 239, 53,
    ],
    [
        118, 39, 26, 78, 19, 125, 210, 50, 4, 229, 108, 57, 87, 122, 16, 246, 116, 162, 16, 9, 211,
        224, 133, 30, 66, 91, 131, 208, 209, 102, 151, 125,
    ],
    [
        183, 194, 145, 8, 164, 192, 176, 214, 186, 1, 11, 149, 45, 204, 235, 178, 201, 16, 24, 83,
        228, 54, 232, 56, 237, 187, 153, 66, 227, 204, 228, 254,
    ],
    [
        227, 138, 221, 233, 143, 189, 36, 125, 49, 135, 151, 54, 216, 218, 196, 30, 62, 129, 22,
        124, 71, 128, 100, 40, 215, 243, 5, 200, 44, 136, 6, 96,
    ],
];