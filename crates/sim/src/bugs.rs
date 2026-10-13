//! Designer bug volumes and the art-lead visual guide.
//! Weak-point centers are the named part in `visual`. Loading fails when that
//! name is missing or does not resolve. Size and the damage multiplier stay
//! on `hitbox`. The mesh crate is not consulted.

use std::collections::BTreeMap;

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
    /// Named points on the cast, in the fitted hitbox frame. A weak-point
    /// anchor may name one. The mesh load rejects a center that is not on
    /// that cast.
    #[serde(default)]
    pub points: BTreeMap<String, [f32; 3]>,
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
    /// Glow mesh radius. Separate from the hitbox radius. Zero on parts that are not a glow.
    #[serde(default)]
    pub visual_radius: f32,
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
    /// Pod indices that sit on the cast instead of the ring. Empty on every other part.
    #[serde(default)]
    pub seats: Vec<PodSeat>,
}

/// One pod center snapped onto the cast. The ring still places every other pod.
#[derive(Clone, Debug, Deserialize)]
pub struct PodSeat {
    pub index: u32,
    pub offset: [f32; 3],
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

/// A glow's place is its weak-point hit center. Eyes are markers, not weak points.
/// The head is never a glow. A glow with no hit center fails the load.
pub fn require_glow_hit_centers(book: &BugBook) -> Result<(), String> {
    for (bug, row) in book.rows() {
        for part in &row.visual.parts {
            if part.name == "head" && (part.emissive || part.visual_radius > 0.0) {
                return Err(format!("{bug} head has no glow"));
            }
            if part.visual_radius <= 0.0 || part.name == "eyes" {
                continue;
            }
            if part.name == "pod" {
                if row.hitbox.pod.is_none() {
                    return Err(format!("{bug} pod glow has no weak point"));
                }
                continue;
            }
            let Some(center) = hit_center_named(row, &part.name) else {
                return Err(format!("{bug} {} glow has no weak point", part.name));
            };
            if part.offset != center {
                return Err(format!(
                    "{bug} {} glow is not on its hit center",
                    part.name
                ));
            }
        }
    }
    Ok(())
}

/// The tail glow is the authored cannon radius. Past `tail_glow_radius`, the load fails.
pub fn require_tail_glow_radius(book: &BugBook, cap: f32) -> Result<(), String> {
    if !(cap > 0.0) {
        return Err("tail glow radius is not positive".into());
    }
    for (bug, row) in book.rows() {
        for part in &row.visual.parts {
            if part.name == "cannon" && part.visual_radius > cap {
                return Err(format!(
                    "{bug} cannon glow radius {:.3} exceeds tail_glow_radius {cap:.3}",
                    part.visual_radius
                ));
            }
        }
    }
    Ok(())
}

fn hit_center_named(row: &BugRow, name: &str) -> Option<[f32; 3]> {
    for (_, shape) in row.hitbox.anchored() {
        let hit_name = if !shape.name.is_empty() {
            shape.name.as_str()
        } else {
            shape.anchor.as_str()
        };
        if hit_name == name {
            return Some(shape.offset);
        }
    }
    None
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

/// Local center of a named cast point or a visual part. `None` when the name is neither.
pub fn visual_anchor(visual: &BugVisual, name: &str) -> Option<[f32; 3]> {
    if let Some(point) = visual.points.get(name) {
        return Some(*point);
    }
    let part = part_named(visual, name)?;
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
    let mut seen = vec![false; count as usize];
    for seat in &pod.seats {
        if seat.index >= count {
            return Err(format!("{bug} pod seat {} is outside the ring", seat.index));
        }
        if seen[seat.index as usize] {
            return Err(format!("{bug} pod seat {} is duplicated", seat.index));
        }
        seen[seat.index as usize] = true;
    }
    for i in 0..count {
        let a = (i as f32 + phase) / count as f32 * std::f32::consts::TAU + turn;
        let nx = a.cos() * pod.ring_radius;
        let nz = a.sin() * pod.ring_radius;
        let ny = (1.0 - nx * nx - nz * nz).max(floor).sqrt();
        let ring = [
            host.offset[0] + nx * radii[0] * sink,
            host.offset[1] + ny * radii[1] * sink,
            host.offset[2] + nz * radii[2] * sink,
        ];
        let offset = pod
            .seats
            .iter()
            .find(|seat| seat.index == i)
            .map(|seat| seat.offset)
            .unwrap_or(ring);
        hitbox.pods.push(PodPlacement { offset });
    }
    Ok(())
}

/// Hit center of a weak point. A missing or unknown anchor name is an error.
pub fn resolved_offset(visual: &BugVisual, shape: &ShapeDef) -> Result<[f32; 3], String> {
    let Some(name) = anchor_name(shape) else {
        return Err("weak point has no anchor name".into());
    };
    visual_anchor(visual, name).ok_or_else(|| format!("anchor \"{name}\" is not on the cast and does not resolve"))
}

/// Volume of the glowing part named in the visual table.
/// A cast point uses the emissive part that sits on that point.
pub fn glow_bounds(visual: &BugVisual, name: &str) -> Option<PartBounds> {
    let center = visual_anchor(visual, name)?;
    let part = part_named(visual, name).or_else(|| {
        visual
            .parts
            .iter()
            .find(|part| part.emissive && part.offset == center)
    })?;
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

