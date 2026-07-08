//! Weapon manifest parsing via RPC.

use bevy::prelude::*;

/// Gun stats parsed from the manifest.
#[derive(Clone, Debug, PartialEq)]
pub struct GunInfo {
    /// Full loadable URL/Path of the model.
    pub path: String,
    pub clip_size: u32,
    pub bullet_type: String,
    pub damage: f32,
    pub range: f32,
    pub rpm: f32,
    pub automatic: bool,
}

/// Melee stats parsed from the manifest.
#[derive(Clone, Debug, PartialEq)]
pub struct MeleeInfo {
    pub path: String,
    pub rpm: f32,
}

/// A selectable weapon.
#[derive(Clone, Debug, PartialEq)]
pub enum WeaponId {
    Unarmed,
    Melee(MeleeInfo),
    Gun(GunInfo),
}

const UNARMED_RPM: f32 = 110.0;

impl WeaponId {
    pub fn is_unarmed(&self) -> bool {
        matches!(self, WeaponId::Unarmed)
    }
    pub fn is_gun(&self) -> bool {
        matches!(self, WeaponId::Gun(_))
    }
    pub fn is_melee(&self) -> bool {
        matches!(self, WeaponId::Melee(_))
    }
    /// The loadable model path, if this weapon has a model.
    pub fn path(&self) -> Option<&str> {
        match self {
            WeaponId::Unarmed => None,
            WeaponId::Melee(m) => Some(&m.path),
            WeaponId::Gun(g) => Some(&g.path),
        }
    }
    /// Attacks per minute (gun fire rate or melee swing rate).
    pub fn rpm(&self) -> f32 {
        match self {
            WeaponId::Unarmed => UNARMED_RPM,
            WeaponId::Melee(m) => m.rpm,
            WeaponId::Gun(g) => g.rpm,
        }
    }
    /// Whether holding LMB continues firing (guns only).
    pub fn automatic(&self) -> bool {
        match self {
            WeaponId::Unarmed | WeaponId::Melee(_) => false,
            WeaponId::Gun(g) => g.automatic,
        }
    }
    /// Gun stats, if this is a gun.
    pub fn gun_info(&self) -> Option<&GunInfo> {
        match self {
            WeaponId::Gun(g) => Some(g),
            _ => None,
        }
    }
    /// A short human-readable label for UI.
    pub fn label(&self) -> String {
        match self.path() {
            None => "Unarmed".to_string(),
            Some(p) => p.rsplit('/').next().unwrap_or(p).replace(".glb", ""),
        }
    }
    pub fn from_label(label: &str, manifest: &WeaponManifest) -> Self {
        for w in &manifest.all {
            if w.label() == label {
                return w.clone();
            }
        }
        WeaponId::Unarmed
    }
}

/// Public manifest resource: the parsed weapon lists plus a combined `all` list (Unarmed first).
#[derive(Resource, Default)]
pub struct WeaponManifest {
    pub guns: Vec<WeaponId>,
    pub melee: Vec<WeaponId>,
    /// `[Unarmed]` + guns + melee, in that order — the order the UI/mouse-wheel cycles through.
    pub all: Vec<WeaponId>,
    pub loaded: bool,
}

#[derive(Resource, Default)]
pub struct WeaponManifestTasks {
    pub manifest_task:
        Option<bevy::tasks::Task<anyhow::Result<game_logic::weapon::WeaponManifestResult>>>,
}

pub fn start_weapon_manifest_load(mut commands: Commands) {
    commands.init_resource::<WeaponManifestTasks>();
}

pub fn spawn_weapon_manifest_task(
    mut tasks: ResMut<WeaponManifestTasks>,
    manifest: Res<WeaponManifest>,
    client: Option<Res<crate::plugins::crack_plugin::CrackClient>>,
) {
    let Some(client) = client else {
        return;
    };
    if !manifest.loaded && tasks.manifest_task.is_none() {
        let api_client = client.0.clone();
        let base_url = crate::config::DATA_BASE_URL.to_string();
        let task = bevy::tasks::AsyncComputeTaskPool::get().spawn(async move {
            api_client
                .call::<game_logic::api::FetchWeaponManifest>(game_logic::api::FetchArgs {
                    base_url,
                })
                .await
        });
        tasks.manifest_task = Some(task);
    }
}

pub fn poll_weapon_manifest_task(
    mut tasks: ResMut<WeaponManifestTasks>,
    mut manifest: ResMut<WeaponManifest>,
) {
    if let Some(mut task) = tasks.manifest_task.take() {
        if let Some(res) = bevy::tasks::futures_lite::future::block_on(
            bevy::tasks::futures_lite::future::poll_once(&mut task),
        ) {
            match res {
                Ok(result) => {
                    let mut guns = Vec::new();
                    let mut melee = Vec::new();
                    for entry in result.weapons {
                        if entry.is_gun {
                            guns.push(WeaponId::Gun(GunInfo {
                                path: entry.path,
                                clip_size: entry.clip_size,
                                bullet_type: entry.bullet_type,
                                damage: entry.damage,
                                range: entry.range,
                                rpm: entry.rpm,
                                automatic: entry.automatic,
                            }));
                        } else {
                            melee.push(WeaponId::Melee(MeleeInfo {
                                path: entry.path,
                                rpm: entry.rpm,
                            }));
                        }
                    }

                    let mut all = vec![WeaponId::Unarmed];
                    all.extend(guns.iter().cloned());
                    all.extend(melee.iter().cloned());
                    manifest.guns = guns;
                    manifest.melee = melee;
                    manifest.all = all;
                    manifest.loaded = true;

                    info!(
                        "Weapon manifest loaded: {} guns, {} melee.",
                        manifest.guns.len(),
                        manifest.melee.len()
                    );
                }
                Err(e) => {
                    tracing::error!("Weapon manifest RPC error: {e:?}");
                }
            }
        } else {
            tasks.manifest_task = Some(task);
        }
    }
}
