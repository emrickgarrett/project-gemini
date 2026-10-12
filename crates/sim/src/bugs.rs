//! Designer bug volumes and the art-lead visual guide.
//! Weak-point centers are the named part in `visual`. Loading fails when that
//! name is missing or does not resolve. Size and the damage multiplier stay
//! on `hitbox`. The mesh crate is not consulted.

use serde::Deserialize;

use crate::content::EnemyKind;
use crate::math::Vec3;

#[derive(Clone, Debug, Deserialize)]
pub struct BugBook {
    /// Weak-point hitbox radius must be at least this times the glowing mesh radius.
    pub hitbox_margin: f32,
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

    pub fn rows(&self) -> [(&str, &BugRow); 5] {
        [
            ("swarmer", &self.swarmer),
            ("spitter", &self.spitter),
            ("brute", &self.brute),
            ("brood_mother", &self.brood_mother),
            ("hive_tyrant", &self.hive_tyrant),
        ]
    }

    fn rows_mut(&mut self) -> [(&str, &mut BugRow); 5] {
        [
            ("swarmer", &mut self.swarmer),
            ("spitter", &mut self.spitter),
            ("brute", &mut self.brute),
            ("brood_mother", &mut self.brood_mother),
            ("hive_tyrant", &mut self.hive_tyrant),
        ]
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
    /// Rest pitch of the first tail segment, radians.
    #[serde(default)]
    pub base_pitch: f32,
    /// Share of `curl_deg / count` applied as pitch on each later segment.
    #[serde(default)]
    pub curl_share: f32,
    /// Child joint, as a fraction of the segment length along local -Y.
    #[serde(default)]
    pub joint_span: f32,
    /// Sculpted segment length, as a fraction of the authored length.
    #[serde(default)]
    pub segment_scale: f32,
    /// Gap between the last segment tip and the cannon gem.
    #[serde(default)]
    pub tip_gap: f32,
    /// Shared segment count cap. Zero means the chain is not authored.
    #[serde(default)]
    pub segment_cap: u32,
    /// Tail sway: `sin(time * sway_rate + index * sway_spread) * sway_amp` added to pitch.
    #[serde(default)]
    pub sway_rate: f32,
    #[serde(default)]
    pub sway_spread: f32,
    #[serde(default)]
    pub sway_amp: f32,
    /// Crack patch, as fractions of the host extents. Empty when this part is not a scar.
    #[serde(default)]
    pub patch: Vec<f32>,
    #[serde(default)]
    pub patch_floor: f32,
    #[serde(default)]
    pub at: Option<String>,
    /// Pod ring. Missing on every part that is not a pod; bake fails when a pod omits them.
    #[serde(default)]
    pub sink: Option<f32>,
    #[serde(default)]
    pub ring_turn: Option<f32>,
    #[serde(default)]
    pub ny_floor: Option<f32>,
    /// "paired" or "radial". Required on a leg. An empty value fails load.
    #[serde(default)]
    pub layout: String,
    /// Radial hip distance, as a fraction of ring_radius.
    #[serde(default)]
    pub hip_scale: Option<f32>,
    /// Radial foot distance, as a fraction of ring_radius.
    #[serde(default)]
    pub plant_scale: Option<f32>,
    /// Radial yaw offset, radians. Pod rings use the same key.
    #[serde(default)]
    pub ring_phase: Option<f32>,
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

    /// Authored pod radius. `None` when this bug has no pod hitbox.
    /// A pod row with no positive radius fails load instead of inventing one.
    pub fn pod_radius(&self) -> Option<f32> {
        self.pod.as_ref().map(|shape| shape.radius)
    }

    /// Weak volumes whose centers must come from a visual part.
    pub fn anchored(&self) -> Vec<(&str, &ShapeDef)> {
        let mut slots = Vec::new();
        for (slot, shape) in [
            ("weakpoint", self.weakpoint.as_ref()),
            ("core", self.core.as_ref()),
            ("cannon", self.cannon.as_ref()),
            ("tail", self.tail.as_ref()),
        ] {
            if let Some(shape) = shape {
                slots.push((slot, shape));
            }
        }
        slots
    }
}

/// Bake every weak-point center from its visual anchor. A missing or unknown
/// name is a content error; the authored offset is not a fallback.
pub fn bind_weak_anchors(book: &mut BugBook) -> Result<(), String> {
    for (bug, row) in book.rows_mut() {
        bind_row(bug, row)?;
    }
    Ok(())
}

/// Every leg names `layout`. Radial legs also name hip scale, plant scale, and phase.
pub fn require_leg_layout(book: &BugBook) -> Result<(), String> {
    for (id, row) in book.rows() {
        let Some(leg) = row.visual.parts.iter().find(|part| part.name == "leg") else {
            return Err(format!("{id} leg is missing layout"));
        };
        if leg.layout.is_empty() {
            return Err(format!("{id} leg is missing layout"));
        }
        match leg.layout.as_str() {
            "paired" => {}
            "radial" => {
                if leg.hip_scale.is_none() {
                    return Err(format!("{id} leg is missing hip_scale"));
                }
                if leg.plant_scale.is_none() {
                    return Err(format!("{id} leg is missing plant_scale"));
                }
                if leg.ring_phase.is_none() {
                    return Err(format!("{id} leg is missing ring_phase"));
                }
            }
            other => return Err(format!("{id} leg layout \"{other}\" is unknown")),
        }
    }
    Ok(())
}

fn bind_row(bug: &str, row: &mut BugRow) -> Result<(), String> {
    let BugRow { visual, hitbox, audio: _ } = row;
    bind_slot(bug, "weakpoint", visual, &mut hitbox.weakpoint)?;
    bind_slot(bug, "core", visual, &mut hitbox.core)?;
    bind_slot(bug, "cannon", visual, &mut hitbox.cannon)?;
    bind_slot(bug, "tail", visual, &mut hitbox.tail)?;
    bake_pods(bug, visual, hitbox)?;
    Ok(())
}

fn bind_slot(bug: &str, slot: &str, visual: &BugVisual, shape: &mut Option<ShapeDef>) -> Result<(), String> {
    let Some(shape) = shape.as_mut() else {
        return Ok(());
    };
    if shape.offset != [0.0, 0.0, 0.0] {
        return Err(format!("{bug} {slot} weak point must not author an offset"));
    }
    let offset = resolved_offset(visual, shape).map_err(|err| format!("{bug} {slot} {err}"))?;
    if shape.anchor.is_empty() {
        shape.anchor.clone_from(&shape.name);
    }
    if shape.name.is_empty() {
        shape.name.clone_from(&shape.anchor);
    }
    shape.offset = offset;
    if shape.active_phase == 0 {
        if let Some(phase) = part_named(visual, &shape.anchor).and_then(|part| part.phase_max) {
            shape.active_phase = phase;
        }
    }
    Ok(())
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
    /// Visual part whose anchor supplies the center. Empty falls through to `name`.
    /// A weak point with neither, or with a name that is not a visual part, fails load.
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

fn anchor_name(shape: &ShapeDef) -> Option<&str> {
    if !shape.anchor.is_empty() {
        Some(shape.anchor.as_str())
    } else if !shape.name.is_empty() {
        Some(shape.name.as_str())
    } else {
        None
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
        let height = size_extents(&host.size)?[1];
        return Some([host.offset[0], host.offset[1] + height * 0.5, host.offset[2]]);
    }
    None
}

/// Pod centers follow the visual pod ring on its host. The gameplay table
/// authors a radius only; an offset list is rejected.
fn bake_pods(bug: &str, visual: &BugVisual, hitbox: &mut BugHitbox) -> Result<(), String> {
    let Some(shape) = hitbox.pod.as_ref() else {
        return Ok(());
    };
    if shape.radius <= 0.0 {
        return Err(format!("{bug} pod radius is missing"));
    }
    if !hitbox.pods.is_empty() {
        return Err(format!("{bug} pod hitboxes must not author offsets"));
    }
    let pod = part_named(visual, "pod").ok_or_else(|| format!("{bug} pod hitbox has no visual pod part"))?;
    let host_name = pod.on.as_deref().ok_or_else(|| format!("{bug} visual pod has no host"))?;
    let host = part_named(visual, host_name).ok_or_else(|| format!("{bug} visual pod host \"{host_name}\" is missing"))?;
    if pod.count == 0 {
        return Err(format!("{bug} visual pod count is missing"));
    }
    if !(pod.ring_radius > 0.0) {
        return Err(format!("{bug} visual pod ring_radius is missing"));
    }
    let ext = size_extents(&host.size).ok_or_else(|| format!("{bug} visual pod host is missing size"))?;
    let sink = pod.sink.ok_or_else(|| format!("{bug} visual pod is missing sink"))?;
    let turn = pod.ring_turn.ok_or_else(|| format!("{bug} visual pod is missing ring_turn"))?;
    let floor = pod.ny_floor.ok_or_else(|| format!("{bug} visual pod is missing ny_floor"))?;
    let radii = [ext[0] * 0.5, ext[1] * 0.5, ext[2] * 0.5];
    let count = pod.count;
    let phase = pod
        .ring_phase
        .ok_or_else(|| format!("{bug} visual pod is missing ring_phase"))?;
    for i in 0..count {
        let a = (i as f32 + phase) / count as f32 * std::f32::consts::TAU + turn;
        let nx = a.cos() * pod.ring_radius;
        let nz = a.sin() * pod.ring_radius;
        let ny = (1.0 - nx * nx - nz * nz).max(floor).sqrt();
        hitbox.pods.push(PodPlacement {
            offset: [
                host.offset[0] + nx * radii[0] * sink,
                host.offset[1] + ny * radii[1] * sink,
                host.offset[2] + nz * radii[2] * sink,
            ],
        });
    }
    Ok(())
}

/// Hit center of a weak point. A missing or unknown anchor name is an error.
pub fn resolved_offset(visual: &BugVisual, shape: &ShapeDef) -> Result<[f32; 3], String> {
    let Some(name) = anchor_name(shape) else {
        return Err("weak point has no anchor name".into());
    };
    visual_anchor(visual, name).ok_or_else(|| format!("anchor \"{name}\" does not resolve to a visual part"))
}

/// Volume of the glowing part named in the visual table.
pub fn glow_bounds(visual: &BugVisual, name: &str) -> Option<PartBounds> {
    let part = part_named(visual, name)?;
    let center = visual_anchor(visual, name)?;
    let half = if part.size.is_empty() {
        let host_name = part.on.as_deref().unwrap_or("");
        let host = part_named(visual, host_name)?;
        let ext = size_extents(&host.size)?;
        let patch = part.patch.as_slice();
        if patch.len() < 3 {
            return None;
        }
        [ext[0] * patch[0], (ext[1] * patch[1]).max(part.patch_floor), ext[2] * patch[2]]
    } else {
        let ext = size_extents(&part.size)?;
        [ext[0] * 0.5, ext[1] * 0.5, ext[2] * 0.5]
    };
    Some(bounds_at(center, half))
}

/// Volume of a hit shape. Weak points already carry the anchor baked at load.
pub fn hit_bounds(shape: &ShapeDef) -> PartBounds {
    let half = match shape.shape.as_str() {
        "box" => shape.half_extents,
        "capsule" => [shape.radius, shape.height * 0.5, shape.radius],
        _ => [shape.radius, shape.radius, shape.radius],
    };
    bounds_at(shape.offset, half)
}

fn bounds_at(center: [f32; 3], half: [f32; 3]) -> PartBounds {
    PartBounds {
        min: [center[0] - half[0], center[1] - half[1], center[2] - half[2]],
        max: [center[0] + half[0], center[1] + half[1], center[2] + half[2]],
    }
}

/// Full extents. Two-number sizes are radius and length, matching the visual table.
fn size_extents(size: &[f32]) -> Option<[f32; 3]> {
    match size {
        [x, y, z, ..] => Some([*x, *y, *z]),
        [r, h] => Some([*r * 2.0, *h, *r * 2.0]),
        [r] => Some([*r, *r, *r]),
        _ => None,
    }
}

/// Cannon center for `at = "tail_end"`. Pitch, joint, segment length, gap, and
/// the segment cap are the tail part's fields. A chain with no cap, scale, or
/// joint is not an anchor.
/// Cannon center at `time`, including the tail's sway. Time 0 is the rest pose baked at load.
pub fn cannon_center(visual: &BugVisual, time: f32) -> Option<[f32; 3]> {
    let cannon = part_named(visual, "cannon")?;
    tail_end_at(visual, cannon, time)
}

fn tail_end(visual: &BugVisual, cannon: &VisualPart) -> Option<[f32; 3]> {
    tail_end_at(visual, cannon, 0.0)
}

fn tail_end_at(visual: &BugVisual, cannon: &VisualPart, time: f32) -> Option<[f32; 3]> {
    let tail = part_named(visual, "tail")?;
    if tail.segment_cap == 0 || tail.segment_scale <= 0.0 || tail.joint_span <= 0.0 {
        return None;
    }
    let len = tail.size.get(1).copied()?;
    let count = tail.count.max(1).min(tail.segment_cap);
    let step = tail.curl_deg.to_radians() / count as f32;

    let mut pos = [0.0_f32, 0.0, 0.0];
    let mut rot = [0.0_f32, 0.0, 0.0, 1.0];
    for i in 0..count {
        let local_pos = if i == 0 {
            tail.offset
        } else {
            [0.0, -len * tail.joint_span, 0.0]
        };
        let sway = (time * tail.sway_rate + i as f32 * tail.sway_spread).sin() * tail.sway_amp;
        let pitch = (if i == 0 { tail.base_pitch } else { step * tail.curl_share }) + sway;
        pos = add3(pos, rotate_q(rot, local_pos));
        rot = quat_mul(rot, quat_x(pitch));
    }
    let gem_r = cannon.size.first().copied().filter(|r| *r > 0.0)? * 0.5;
    let along = [0.0, -(len * tail.segment_scale + gem_r + tail.tip_gap), 0.0];
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
