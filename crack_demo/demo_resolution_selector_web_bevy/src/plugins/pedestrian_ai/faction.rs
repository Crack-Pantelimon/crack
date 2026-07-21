//! Faction roster, war matrix, and health model for the pedestrian AI.

use bevy::prelude::*;

/// Default hit points for AI pedestrians.
pub const DEFAULT_HP: f32 = 100.0;

/// How long a corpse stays around playing its death clip before it despawns.
pub const DEATH_ANIM_TIME: f32 = 2.5;

/// Static faction roster. `Neutral` never fights and is never targeted.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Faction {
/// neutral variant.
    Neutral,
/// red variant.
    Red,
/// green variant.
    Green,
/// blue variant.
    Blue,
/// yellow variant.
    Yellow,
}

impl Faction {
    /// All factions that actively participate in combat.
    pub const COMBATANTS: [Faction; 4] =
        [Faction::Red, Faction::Green, Faction::Blue, Faction::Yellow];

    /// Gizmo/tint color for this faction.
    pub fn color(self) -> Color {
        match self {
            Faction::Neutral => Color::srgb(0.6, 0.6, 0.6),
            Faction::Red => Color::srgb(1.0, 0.2, 0.2),
            Faction::Green => Color::srgb(0.2, 1.0, 0.2),
            Faction::Blue => Color::srgb(0.3, 0.4, 1.0),
            Faction::Yellow => Color::srgb(1.0, 0.9, 0.2),
        }
    }

/// label.
    pub fn label(self) -> &'static str {
        match self {
            Faction::Neutral => "Neutral",
            Faction::Red => "Red",
            Faction::Green => "Green",
            Faction::Blue => "Blue",
            Faction::Yellow => "Yellow",
        }
    }
}

/// Static war matrix resource. `wars` holds unordered pairs of warring factions.
#[derive(Resource, Default)]
pub struct WarMatrix {
/// wars field.
    pub wars: Vec<(Faction, Faction)>,
}

impl WarMatrix {
    /// Every distinct combatant pair at war with each other.
    pub fn all_out_war() -> Self {
        let mut wars = Vec::new();
        let c = Faction::COMBATANTS;
        for i in 0..c.len() {
            for j in (i + 1)..c.len() {
                wars.push((c[i], c[j]));
            }
        }
        Self { wars }
    }

    /// A partial rivalry map: only *some* faction pairs are at war, so the streets have a mix of
    /// friendly and hostile clans. Red feuds with Blue, and Green feuds with Yellow; the other
    /// cross-pairs coexist peacefully.
    pub fn gang_wars() -> Self {
        Self {
            wars: vec![
                (Faction::Red, Faction::Blue),
                (Faction::Green, Faction::Yellow),
            ],
        }
    }

    /// Returns true if factions `a` and `b` are at war.
    pub fn at_war(&self, a: Faction, b: Faction) -> bool {
        if a == Faction::Neutral || b == Faction::Neutral || a == b {
            return false;
        }
        self.wars
            .iter()
            .any(|&(x, y)| (x == a && y == b) || (x == b && y == a))
    }
}

/// Hit points. Death handled centrally when `current <= 0`.
#[derive(Component, Clone, Copy)]
pub struct Health {
/// current field.
    pub current: f32,
/// max field.
    pub max: f32,
}

impl Health {
/// full.
    pub fn full(max: f32) -> Self {
        Self { current: max, max }
    }
}

/// Personal grudge list: entities that have damaged this pedestrian (by weapon or by running it
/// over with a car). An AI ped will attack anyone on this list regardless of faction. Entries are
/// pruned when the referenced entity dies ([`super::PedestrianDied`]) or no longer resolves.
#[derive(Component, Default)]
pub struct Enemies(pub Vec<Entity>);

impl Enemies {
    /// Add `who` as a personal enemy (deduplicated).
    pub fn insert(&mut self, who: Entity) {
        if !self.0.contains(&who) {
            self.0.push(who);
        }
    }
}

/// Marks a pedestrian (AI or player) that has died: it is playing its death clip and will be
/// despawned once `timer` counts down to zero. While present, all AI systems ignore the entity
/// (they already skip anything with `Health::current <= 0`).
#[derive(Component)]
pub struct Dying {
    /// Seconds left before the corpse is despawned.
    pub timer: f32,
}
