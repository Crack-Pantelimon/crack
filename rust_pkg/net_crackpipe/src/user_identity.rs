use iroh::{PublicKey, SecretKey};

/// User identity derived from a public key.
/// Provides nickname, color, and access to the underlying public key.
#[derive(
    Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub struct UserIdentity {
    user_id: PublicKey,
}

impl UserIdentity {
    /// Returns a deterministic nickname generated from the public key.
    pub fn nickname(&self) -> String {
        crate::_random_word::get_nickname_from_pubkey(self.user_id)
    }
    /// Creates a `UserIdentity` from an existing public key.
    pub const fn from_userid(user_id: PublicKey) -> Self {
        Self { user_id }
    }
    /// Returns the underlying public key (user ID).
    pub fn user_id(&self) -> &PublicKey {
        &self.user_id
    }
    /// Returns the color as an HTML rgb() string.
    pub fn html_color(&self) -> String {
        let color = self.rgb_color();
        format!("rgb({},{},{})", color.0, color.1, color.2)
    }
    /// Returns an RGB color tuple derived from the public key.
    pub fn rgb_color(&self) -> (u8, u8, u8) {
        let pubkey_bytes = self.user_id.as_bytes();
        let mut color = [0_u8; 3];
        for (i, value) in pubkey_bytes.iter().enumerate().take(32) {
            let k = i % 3;
            color[k] ^= value;
        }
        (color[0], color[1], color[2])
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
/// User identity with secret key for signing.
/// Contains the private key and derived public identity.
pub struct UserIdentitySecrets {
    _user_private_key: SecretKey,
    user_identity: UserIdentity,
}

impl PartialEq for UserIdentitySecrets {
    fn eq(&self, other: &Self) -> bool {
        self.user_identity == other.user_identity
            && self._user_private_key.public() == other._user_private_key.public()
    }
}

impl UserIdentitySecrets {
    /// Returns a reference to the public user identity.
    pub fn user_identity(&self) -> &UserIdentity {
        &self.user_identity
    }
    /// Returns a reference to the secret key for signing.
    pub fn secret_key(&self) -> &SecretKey {
        &self._user_private_key
    }
    /// Generates a new random identity with a fresh secret key.
    pub fn generate() -> Self {
        let _user_private_key = SecretKey::generate(rand::thread_rng());
        let user_id = _user_private_key.public();
        let user_identity = UserIdentity { user_id };
        Self {
            _user_private_key,
            user_identity,
        }
    }
}

#[derive(
    Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, PartialOrd, Ord, Eq, Hash,
)]
/// Node identity combining user identity, node ID, and optional bootstrap index.
/// Used to identify nodes in the network with display info.
pub struct NodeIdentity {
    user_identity: UserIdentity,
    node_id: PublicKey,
    bootstrap_idx: Option<u32>,
}

impl NodeIdentity {
    /// Returns the display nickname, with bootstrap index if applicable.
    pub fn nickname(&self) -> String {
        if let Some(bootstrap_idx) = self.bootstrap_idx {
            format!(
                "{} (bootstrap #{})",
                self.user_identity.nickname(),
                bootstrap_idx
            )
        } else {
            self.user_identity.nickname().to_string()
        }
    }
    /// Returns the color as an HTML rgb() string.
    pub fn html_color(&self) -> String {
        self.user_identity.html_color()
    }
    /// Returns an RGB color tuple derived from the user identity.
    pub fn rgb_color(&self) -> (u8, u8, u8) {
        self.user_identity.rgb_color()
    }
    /// Returns the user's public key (user ID).
    pub fn user_id(&self) -> &PublicKey {
        self.user_identity.user_id()
    }
    /// Returns this node's public key (node ID).
    pub fn node_id(&self) -> &PublicKey {
        &self.node_id
    }
    /// Returns a reference to the user identity.
    pub fn user_identity(&self) -> &UserIdentity {
        &self.user_identity
    }
    /// Returns the bootstrap index if this is a bootstrap node.
    pub fn bootstrap_idx(&self) -> Option<u32> {
        self.bootstrap_idx
    }

    /// Creates a new node identity from parts.
    pub fn new(
        user_identity: UserIdentity,
        node_id: PublicKey,
        bootstrap_idx: Option<u32>,
    ) -> Self {
        Self {
            user_identity,
            node_id,
            bootstrap_idx,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_arch = "wasm32")]
    use wasm_bindgen_test::wasm_bindgen_test as test;

    #[test]
    fn smoke_generate() {
        let secrets = UserIdentitySecrets::generate();
        // The public identity must match the generated secret key.
        assert_eq!(
            secrets.secret_key().public(),
            *secrets.user_identity().user_id()
        );
        // Two generations must not collide (guards against a stubbed RNG).
        let other = UserIdentitySecrets::generate();
        assert_ne!(secrets, other);
    }
}
