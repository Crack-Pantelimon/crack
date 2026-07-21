//! Weapon manifest parsing via RPC.

use bevy::prelude::*;

/// Gun stats parsed from the manifest.
#[derive(Clone, Debug, PartialEq)]
pub struct GunInfo {
    /// Full loadable URL/Path of the model.
    pub path: String,
/// clip size field.
    pub clip_size: u32,
/// bullet type field.
    pub bullet_type: String,
/// damage field.
    pub damage: f32,
/// range field.
    pub range: f32,
/// rpm field.
    pub rpm: f32,
/// automatic field.
    pub automatic: bool,
/// reload secs field.
    pub reload_secs: f32,
}

/// Melee stats parsed from the manifest.
#[derive(Clone, Debug, PartialEq)]
pub struct MeleeInfo {
/// path field.
    pub path: String,
/// rpm field.
    pub rpm: f32,
}

/// A selectable weapon.
#[derive(Clone, Debug, PartialEq)]
pub enum WeaponId {
/// unarmed variant.
    Unarmed,
/// Documented public item.
    Melee(MeleeInfo),
/// Documented public item.
    Gun(GunInfo),
}

const UNARMED_RPM: f32 = 110.0;

fn weapon_basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Hand-picked reload durations keyed on weapon file name (overrides CSV/default).
fn hand_picked_reload_secs(path: &str) -> Option<f32> {
    match weapon_basename(path) {
        "revolver1.glb" | "revolver2.glb" | "revolver3.glb" => Some(3.0),
        "pistol1-cv.glb"
        | "pistol2-glock.glb"
        | "pistol3-fallout.glb"
        | "pistol4-cz.glb"
        | "pistol5-1911.glb"
        | "pistol6-breta.glb"
        | "pistol7-tt.glb"
        | "pistol8-luger.glb" => Some(1.6),
        "uzi1.glb" | "uzi2.glb" => Some(2.0),
        "mp5-mini.glb" | "mp5-mini-2.glb" | "skorpion1.glb" => Some(2.2),
        "ak47.glb" => Some(2.8),
        "draco1.glb" | "draco2.glb" => Some(2.5),
        _ => None,
    }
}

fn resolve_gun_reload_secs(path: &str, csv_secs: f32) -> f32 {
    hand_picked_reload_secs(path).unwrap_or(csv_secs)
}

impl WeaponId {
/// is unarmed.
    pub fn is_unarmed(&self) -> bool {
        matches!(self, WeaponId::Unarmed)
    }
/// is gun.
    pub fn is_gun(&self) -> bool {
        matches!(self, WeaponId::Gun(_))
    }
/// is melee.
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
    /// Whether holding LMB continues firing.
    pub fn automatic(&self) -> bool {
        match self {
            WeaponId::Unarmed | WeaponId::Melee(_) => true,
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
/// from label.
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
/// guns field.
    pub guns: Vec<WeaponId>,
/// melee field.
    pub melee: Vec<WeaponId>,
    /// `[Unarmed]` + guns + melee, in that order — the order the UI/mouse-wheel cycles through.
    pub all: Vec<WeaponId>,
/// loaded field.
    pub loaded: bool,
}

/// weapon manifest tasks.
#[derive(Resource, Default)]
pub struct WeaponManifestTasks {
/// manifest task field.
    pub manifest_task:
        Option<bevy::tasks::Task<anyhow::Result<game_logic::weapon::WeaponManifestResult>>>,
}

/// start weapon manifest load.
pub fn start_weapon_manifest_load(mut commands: Commands) {
    commands.init_resource::<WeaponManifestTasks>();
}

/// spawn weapon manifest task.
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

/// poll weapon manifest task.
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
                                path: entry.path.clone(),
                                clip_size: entry.clip_size,
                                bullet_type: entry.bullet_type,
                                damage: entry.damage,
                                range: entry.range,
                                rpm: entry.rpm,
                                automatic: entry.automatic,
                                reload_secs: resolve_gun_reload_secs(
                                    &entry.path,
                                    entry.reload_secs,
                                ),
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
