//! Code-driven joints. The sim never sees this; footsteps are a render-side plant.

use crate::visual::{EnemyVisual, Gate, Joint, Part};

#[derive(Clone, Copy, Debug)]
pub struct AnimInput {
    pub time: f32,
    pub moving: bool,
    pub charging: bool,
    pub vulnerable: bool,
    pub phase: u32,
    pub frenzy: bool,
    /// Stills only. Phase-min glow (hive veins) draws without hiding phase-max parts.
    pub preview_glow: bool,
    /// How many brood buds have grown into pods. The sim never reads this.
    pub regrow: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct Xform {
    pub pos: [f32; 3],
    pub rot: [f32; 4],
    pub scale: [f32; 3],
    pub visible: bool,
    /// Emissive multiplier. Charge glow drops back to 1 the frame charging ends.
    pub glow: f32,
}

#[derive(Clone, Debug)]
pub struct Pose {
    pub xforms: Vec<Xform>,
    /// Per-foot lift, 0 when the foot is planted.
    pub foot_lift: Vec<f32>,
    pub planted: Vec<u32>,
}

pub fn pose(visual: &EnemyVisual, anim: AnimInput, prev_lift: &[f32]) -> Pose {
    let n = visual.parts.len();
    let mut locals: Vec<([f32; 3], [f32; 4], [f32; 3], bool, f32)> = Vec::with_capacity(n);
    let mut lifts = Vec::new();
    let rate = if anim.frenzy {
        visual.frenzy_speed.max(1.0)
    } else {
        1.0
    };
    let move_amt = if anim.moving { 1.0 } else { 0.0 };
    let bob = (anim.time * visual.bob_hz * std::f32::consts::TAU).sin() * visual.bob_amp * (0.35 + 0.65 * move_amt);
    let lean = if anim.charging {
        -visual.charge_lean_deg.to_radians()
    } else {
        0.0
    };

    for part in &visual.parts {
        let mut euler = part.rest_euler;
        let mut pos = part.rest_pos;
        let mut scale = [1.0_f32, 1.0, 1.0];
        let mut glow = if part.emissive { 1.0 } else { 0.0 };
        match part.joint {
            Joint::Leg { index, segment } => {
                let phase = leg_phase(visual, index, anim.time, rate);
                let swing = visual.leg_swing_deg.to_radians() * move_amt * phase.sin();
                if segment == 0 {
                    euler[0] += swing;
                } else {
                    euler[0] -= swing * 0.8;
                }
            }
            Joint::Foot { index } => {
                let phase = leg_phase(visual, index, anim.time, rate);
                let lift = if anim.moving { phase.sin().max(0.0) } else { 0.0 };
                pos[1] += lift * 0.08;
                while lifts.len() <= index as usize {
                    lifts.push(0.0);
                }
                lifts[index as usize] = lift;
            }
            Joint::Mandible { side } => {
                let snap = (anim.time * part.swing_hz).sin().abs();
                euler[1] += side.signum() * snap * part.swing_amp;
            }
            Joint::Sac => {
                let breathe = if visual.sac_breathe[1] > 0.0 {
                    (anim.time * visual.sac_breathe[1] * std::f32::consts::TAU).sin() * visual.sac_breathe[0]
                } else {
                    0.0
                };
                let charge = if anim.charging && visual.sac_charge_scale > 1.0 {
                    visual.sac_charge_scale
                } else {
                    1.0
                };
                let s = charge + breathe;
                scale = [s, s, s];
                if part.emissive {
                    glow = if anim.charging && visual.sac_charge_glow > 0.0 {
                        visual.sac_charge_glow
                    } else if visual.glow_pulse > 0.0 {
                        1.0 + 0.15 * (anim.time * visual.glow_pulse).sin()
                    } else {
                        1.0
                    };
                }
            }
            Joint::Claw { side } => {
                euler[0] += (anim.time * part.swing_hz).sin() * part.swing_amp * move_amt * side.signum();
            }
            Joint::Scythe { side } => {
                euler[0] += (anim.time * part.swing_hz).sin() * part.swing_amp * side.signum();
            }
            Joint::Tail { index } => {
                euler[0] += (anim.time * part.sway_rate + index as f32 * part.sway_spread).sin() * part.sway_amp;
            }
            Joint::Static => {
                if part.emissive && visual.glow_pulse > 0.0 {
                    glow = 1.0 + 0.12 * (anim.time * visual.glow_pulse).sin();
                }
            }
        }
        // Dull sac-colored buds swell to pod size and take the pod glow on regrow.
        if let Some(slot) = part.grow_slot {
            if slot < anim.regrow {
                let grown = part.grown_scale;
                scale = [grown, grown, grown];
                glow = part.grown_glow;
            } else {
                glow = 0.0;
            }
        }
        if part.parent.is_none() {
            pos[1] += bob;
            euler[0] += lean;
        }
        let visible = gate_open(part, &anim);
        if !visible {
            glow = 0.0;
        }
        locals.push((pos, quat_from_euler_xyz(euler[0], euler[1], euler[2]), scale, visible, glow));
    }

    let mut cache: Vec<Option<Xform>> = vec![None; n];
    let mut xforms = Vec::with_capacity(n);
    for i in 0..n {
        xforms.push(world(i, &visual.parts, &locals, &mut cache));
    }
    let mut planted = Vec::new();
    if anim.moving {
        for (i, lift) in lifts.iter().enumerate() {
            let prev = prev_lift.get(i).copied().unwrap_or(0.0);
            if prev > 0.04 && *lift <= 0.04 {
                planted.push(i as u32);
            }
        }
    }
    Pose {
        xforms,
        foot_lift: lifts,
        planted,
    }
}

fn gate_open(part: &Part, anim: &AnimInput) -> bool {
    match part.gate {
        Gate::Always => true,
        Gate::Vulnerable => anim.vulnerable,
        Gate::PhaseMax(max) => anim.phase + 1 <= max,
        Gate::PhaseMin(min) => anim.preview_glow || anim.phase + 1 >= min,
    }
}

fn leg_phase(visual: &EnemyVisual, index: u32, time: f32, rate: f32) -> f32 {
    let base = time * visual.bob_hz.max(0.2) * std::f32::consts::TAU * rate;
    let offset = if visual.gait == "tripod" {
        let left = index % 2 == 0;
        let row = index / 2;
        if left == (row % 2 == 0) {
            0.0
        } else {
            std::f32::consts::PI
        }
    } else if index % 2 == 0 {
        0.0
    } else {
        std::f32::consts::PI
    };
    base + offset
}

fn world(
    i: usize,
    parts: &[Part],
    locals: &[([f32; 3], [f32; 4], [f32; 3], bool, f32)],
    cache: &mut [Option<Xform>],
) -> Xform {
    if let Some(xf) = cache[i] {
        return xf;
    }
    let (pos, rot, scale, visible, glow) = locals[i];
    let xf = if let Some(p) = parts[i].parent {
        let pw = world(p, parts, locals, cache);
        let scaled = [pos[0] * pw.scale[0], pos[1] * pw.scale[1], pos[2] * pw.scale[2]];
        let rotated = rotate_vec(pw.rot, scaled);
        Xform {
            pos: [pw.pos[0] + rotated[0], pw.pos[1] + rotated[1], pw.pos[2] + rotated[2]],
            rot: quat_mul(pw.rot, rot),
            scale: [pw.scale[0] * scale[0], pw.scale[1] * scale[1], pw.scale[2] * scale[2]],
            visible: visible && pw.visible,
            glow,
        }
    } else {
        Xform {
            pos,
            rot,
            scale,
            visible,
            glow,
        }
    };
    cache[i] = Some(xf);
    xf
}

pub fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [ax, ay, az, aw] = a;
    let [bx, by, bz, bw] = b;
    [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
    ]
}

pub fn quat_from_euler_xyz(x: f32, y: f32, z: f32) -> [f32; 4] {
    let (sx, cx) = (x * 0.5).sin_cos();
    let (sy, cy) = (y * 0.5).sin_cos();
    let (sz, cz) = (z * 0.5).sin_cos();
    let qx = [sx, 0.0, 0.0, cx];
    let qy = [0.0, sy, 0.0, cy];
    let qz = [0.0, 0.0, sz, cz];
    quat_mul(quat_mul(qx, qy), qz)
}

pub fn rotate_vec(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let qv = [v[0], v[1], v[2], 0.0];
    let inv = [-q[0], -q[1], -q[2], q[3]];
    let r = quat_mul(quat_mul(q, qv), inv);
    [r[0], r[1], r[2]]
}
