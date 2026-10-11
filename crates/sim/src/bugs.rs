//! Designer bug volumes and the art-lead visual guide.
//! Weak-point centers come from the named part in `visual`. Size and the
//! damage multiplier stay on `hitbox`; its offset is the fallback. The mesh
//! crate is not consulted.

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
    /// Rest palette key for buds. Empty when this bug has none.
    #[serde(default)]
    pub bud_color: String,
    /// Palette key a bud takes once it grows into a pod.
    #[serde(default)]
    pub grown_color: String,
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
    /// Hive cannon. Phase comes from the named visual part when `active_phase` is unset.
    #[serde(default)]
    pub cannon: Option<ShapeDef>,
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
    /// Visual part whose anchor supplies the center. Empty keeps `offset`.
    #[serde(default)]
    pub anchor: String,
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

/// Axis-aligned volume in the bug's local frame.
#[derive(Clone, Copy, Debug)]
pub struct PartBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl PartBounds {
    pub fn center(self) -> [f32; 3] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
            (self.min[2] + self.max[2]) * 0.5,
        ]
    }

    pub fn overlaps(self, other: Self) -> bool {
        self.min[0] <= other.max[0]
            && self.max[0] >= other.min[0]
            && self.min[1] <= other.max[1]
            && self.max[1] >= other.min[1]
            && self.min[2] <= other.max[2]
            && self.max[2] >= other.min[2]
    }
}

pub fn part_named<'a>(visual: &'a BugVisual, name: &str) -> Option<&'a VisualPart> {
    visual.parts.iter().find(|part| part.name == name)
}

fn part_key<'a>(shape: &'a ShapeDef, implicit: &'a str) -> &'a str {
    if !shape.anchor.is_empty() {
        shape.anchor.as_str()
    } else if !shape.name.is_empty() {
        shape.name.as_str()
    } else {
        implicit
    }
}

/// Local center of a visual part. `None` when the part is missing or has no anchor.
pub fn visual_anchor(visual: &BugVisual, name: &str) -> Option<[f32; 3]> {
    let part = part_named(visual, name)?;
    if part.at.as_deref() == Some("tail_end") {
        return tail_end(visual, part);
    }
    // `[0, 0, 0]` is the serde default for an omitted offset, not a placed part.
    if part.offset != [0.0, 0.0, 0.0] {
        return Some(part.offset);
    }
    if let Some(host_name) = part.on.as_deref() {
        let host = part_named(visual, host_name)?;
        let height = size_extents(&host.size)[1];
        return Some([host.offset[0], host.offset[1] + height * 0.5, host.offset[2]]);
    }
    None
}

/// Hit center: the named part's anchor, or the gameplay offset when that part has none.
pub fn resolved_offset(visual: &BugVisual, shape: &ShapeDef, implicit: &str) -> [f32; 3] {
    let name = part_key(shape, implicit);
    if !name.is_empty() {
        if let Some(anchor) = visual_anchor(visual, name) {
            return anchor;
        }
    }
    shape.offset
}

/// Copy of `shape` with its center (and, when unset, its phase) taken from the visual part.
pub fn place_shape(visual: &BugVisual, shape: &ShapeDef, implicit: &str) -> ShapeDef {
    let mut placed = shape.clone();
    placed.offset = resolved_offset(visual, shape, implicit);
    if placed.active_phase == 0 {
        let name = part_key(shape, implicit);
        if let Some(phase) = part_named(visual, name).and_then(|part| part.phase_max) {
            placed.active_phase = phase;
        }
    }
    placed
}

/// Volume of the glowing part named in the visual table.
pub fn glow_bounds(visual: &BugVisual, name: &str) -> Option<PartBounds> {
    let part = part_named(visual, name)?;
    let center = visual_anchor(visual, name)?;
    let half = if part.size.is_empty() {
        let host_name = part.on.as_deref().unwrap_or("");
        let host = part_named(visual, host_name)?;
        let ext = size_extents(&host.size);
        // Scars sit on the host's crown. The slab is the glowing patch, not the whole shell.
        [ext[0] * 0.28, (ext[1] * 0.1).max(0.05), ext[2] * 0.22]
    } else {
        let ext = size_extents(&part.size);
        [ext[0] * 0.5, ext[1] * 0.5, ext[2] * 0.5]
    };
    Some(bounds_at(center, half))
}

/// Volume of a hit shape after its center is resolved from the visual anchor.
pub fn hit_bounds(visual: &BugVisual, shape: &ShapeDef, implicit: &str) -> PartBounds {
    let placed = place_shape(visual, shape, implicit);
    let half = match placed.shape.as_str() {
        "box" => placed.half_extents,
        "capsule" => [placed.radius, placed.height * 0.5, placed.radius],
        _ => [placed.radius, placed.radius, placed.radius],
    };
    bounds_at(placed.offset, half)
}

fn bounds_at(center: [f32; 3], half: [f32; 3]) -> PartBounds {
    PartBounds {
        min: [center[0] - half[0], center[1] - half[1], center[2] - half[2]],
        max: [center[0] + half[0], center[1] + half[1], center[2] + half[2]],
    }
}

/// Full extents. Two-number sizes are radius and length, matching the visual table.
fn size_extents(size: &[f32]) -> [f32; 3] {
    match size {
        [x, y, z, ..] => [*x, *y, *z],
        [r, h] => [*r * 2.0, *h, *r * 2.0],
        [r] => [*r, *r, *r],
        _ => [0.2, 0.2, 0.2],
    }
}

/// Cannon center for `at = "tail_end"`. The chain is read from the tail part:
/// the limb points along local -Y, the first pitch swings it over the back,
/// and `curl_deg` is shared across `count` segments.
fn tail_end(visual: &BugVisual, cannon: &VisualPart) -> Option<[f32; 3]> {
    let tail = part_named(visual, "tail")?;
    let count = tail.count.max(1);
    let len = tail.size.get(1).copied().unwrap_or(0.8);
    let step = tail.curl_deg.to_radians() / count as f32;
    // Chain convention shared with the sculpt. These are not mesh imports.
    const BASE_PITCH: f32 = 2.05;
    const CURL_SHARE: f32 = 0.70;
    const JOINT: f32 = 0.82;
    const SEGMENT: f32 = 0.90;
    const GAP: f32 = 0.02;

    let mut pos = [0.0_f32, 0.0, 0.0];
    let mut rot = [0.0_f32, 0.0, 0.0, 1.0];
    for i in 0..count {
        let local_pos = if i == 0 { tail.offset } else { [0.0, -len * JOINT, 0.0] };
        let pitch = if i == 0 { BASE_PITCH } else { step * CURL_SHARE };
        pos = add3(pos, rotate_q(rot, local_pos));
        rot = quat_mul(rot, quat_x(pitch));
    }
    let gem_r = cannon.size.first().copied().unwrap_or(0.7) * 0.5;
    let along = [0.0, -(len * SEGMENT + gem_r + GAP), 0.0];
    Some(add3(pos, rotate_q(rot, along)))
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn quat_x(angle: f32) -> [f32; 4] {
    let (s, c) = (angle * 0.5).sin_cos();
    [s, 0.0, 0.0, c]
}

fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

fn rotate_q(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let qv = [v[0], v[1], v[2], 0.0];
    let inv = [-q[0], -q[1], -q[2], q[3]];
    let r = quat_mul(quat_mul(q, qv), inv);
    [r[0], r[1], r[2]]
}
