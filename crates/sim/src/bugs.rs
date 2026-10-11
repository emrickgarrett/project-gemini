//! Designer bug volumes and the art-lead visual guide.
//! Hit registration reads `hitbox` only. The visual block is data for the mesh.

use serde::Deserialize;

use crate::content::EnemyKind;
use crate::math::Vec3;

#[derive(Clone, Debug, Deserialize)]
pub struct BugBook {
    pub feedback: Feedback,
    pub swarmer: BugRow,
    pub spitter: BugRow,
    pub brute: BugRow,
    pub brood_mother: BugRow,
    pub hive_tyrant: BugRow,
}

impl BugBook {
    pub fn of(&self, kind: EnemyKind) -> &BugRow {
        match kind {
            EnemyKind::Swarmer => &self.swarmer,
            EnemyKind::Spitter => &self.spitter,
            EnemyKind::Brute => &self.brute,
            EnemyKind::BroodMother => &self.brood_mother,
            EnemyKind::HiveTyrant => &self.hive_tyrant,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Feedback {
    pub weakpoint_hit: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BugRow {
    pub visual: BugVisual,
    pub hitbox: BugHitbox,
    pub audio: BugAudio,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BugVisual {
    pub height: f32,
    #[serde(default)]
    pub lod_distance: f32,
    #[serde(default)]
    pub parts: Vec<VisualPart>,
    pub anim: VisualAnim,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VisualPart {
    pub name: String,
    #[serde(default)]
    pub shape: String,
    #[serde(default)]
    pub size: Vec<f32>,
    #[serde(default)]
    pub offset: [f32; 3],
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub emissive: bool,
    #[serde(default)]
    pub mirror: bool,
    #[serde(default)]
    pub count: u32,
    #[serde(default)]
    pub ring_radius: f32,
    #[serde(default)]
    pub tilt_deg: f32,
    #[serde(default)]
    pub on: Option<String>,
    #[serde(default)]
    pub active_when: Option<String>,
    #[serde(default)]
    pub phase_max: Option<u32>,
    #[serde(default)]
    pub chain: bool,
    #[serde(default)]
    pub curl_deg: f32,
    #[serde(default)]
    pub at: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct VisualAnim {
    #[serde(default)]
    pub bob: [f32; 2],
    #[serde(default)]
    pub leg_swing: f32,
    #[serde(default)]
    pub gait: String,
    #[serde(default)]
    pub glow_pulse: f32,
    #[serde(default)]
    pub sac_charge_scale: f32,
    #[serde(default)]
    pub sac_charge_glow: f32,
    #[serde(default)]
    pub charge_lean_deg: f32,
    #[serde(default)]
    pub pod_charge_glow: f32,
    #[serde(default)]
    pub pod_charge_time: f32,
    #[serde(default)]
    pub sac_breathe: [f32; 2],
    #[serde(default)]
    pub frenzy_speed: f32,
    #[serde(default)]
    pub phase_burst: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct BugHitbox {
    pub body: ShapeDef,
    #[serde(default)]
    pub weakpoint: Option<ShapeDef>,
    #[serde(default)]
    pub core: Option<ShapeDef>,
    #[serde(default)]
    pub tail: Option<ShapeDef>,
    #[serde(default)]
    pub pod: Option<ShapeDef>,
    #[serde(default)]
    pub pods: Vec<PodPlacement>,
    #[serde(default)]
    pub sac_pop_cancels_shot: bool,
    #[serde(default)]
    pub armor_mult_when_closed: f32,
    #[serde(default)]
    pub vulnerable_after_charge: f32,
    #[serde(default)]
    pub charge_seconds: f32,
}

impl BugHitbox {
    /// The glowing weak volume: the named weakpoint, or the hive core.
    pub fn weak(&self) -> Option<&ShapeDef> {
        self.weakpoint.as_ref().or(self.core.as_ref())
    }

    pub fn pod_radius(&self) -> f32 {
        self.pod.as_ref().map(|p| p.radius).unwrap_or(0.35)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct PodPlacement {
    pub offset: [f32; 3],
}

#[derive(Clone, Debug, Deserialize)]
pub struct ShapeDef {
    #[serde(default)]
    pub name: String,
    #[serde(default = "sphere_name")]
    pub shape: String,
    #[serde(default)]
    pub radius: f32,
    #[serde(default)]
    pub height: f32,
    #[serde(default)]
    pub half_extents: [f32; 3],
    #[serde(default)]
    pub offset: [f32; 3],
    #[serde(default)]
    pub mult: f32,
    #[serde(default)]
    pub grows_during_charge: f32,
    #[serde(default)]
    pub active_only_when: String,
    /// Design-phase number. 0 means always. 1 is the first phase (sim index 0).
    #[serde(default)]
    pub active_phase: u32,
}

fn sphere_name() -> String {
    "sphere".into()
}

impl ShapeDef {
    pub fn offset_vec(&self) -> Vec3 {
        Vec3::new(self.offset[0], self.offset[1], self.offset[2])
    }

    pub fn bounding_radius(&self) -> f32 {
        match self.shape.as_str() {
            "box" => {
                let h = self.half_extents;
                (h[0] * h[0] + h[1] * h[1] + h[2] * h[2]).sqrt()
            }
            "capsule" => self.radius + self.height * 0.5,
            _ => self.radius,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct BugAudio {
    pub footstep: String,
    #[serde(default)]
    pub attack: String,
    #[serde(default)]
    pub charge: String,
    #[serde(default)]
    pub fire: String,
    #[serde(default)]
    pub sac_pop: String,
    #[serde(default)]
    pub shell_open: String,
}

/// Design phase 1 is sim phase 0.
pub fn phase_allows(active_phase: u32, sim_phase: u8) -> bool {
    active_phase == 0 || active_phase == sim_phase as u32 + 1
}
