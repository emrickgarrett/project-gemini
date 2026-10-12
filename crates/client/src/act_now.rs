//! Act-now cues. The client derives them from sim state it already has.
//! Nothing here names a hero or an ability id. Callers pass the local ability's
//! cooldown and infinite-magazine flag from that hero's content row.

use bevy::prelude::*;
use gemini_sim::content::{PlayedAbility, Warnings};
use gemini_sim::sim::{Player, Sim};

use crate::audio::UiRequest;

const REVIVE_START: &str = "revive_start";
const LOW_HP_WARN: &str = "low_hp_warn";
const AMMO_LOW_WARN: &str = "ammo_low_warn";
const ABILITY_READY: &str = "ability_ready";

#[derive(Clone, Debug)]
pub struct Seat {
    pub token: u64,
    pub hp: f32,
    pub max_hp: f32,
    pub downed: bool,
    pub alive: bool,
    pub ammo: u32,
    pub mag: u32,
    pub ability_cd: f32,
    pub ability_active: bool,
    pub revive: f32,
    pub x: f32,
    pub z: f32,
}

pub fn seats(sim: &Sim) -> Vec<Seat> {
    sim.players
        .iter()
        .map(|player| Seat {
            token: player.token,
            hp: player.hp,
            max_hp: player.max_hp,
            downed: player.downed,
            alive: player.alive,
            ammo: active_gun(player).0,
            mag: active_gun(player).1,
            ability_cd: player.ability_cd,
            ability_active: player.overdrive_left > 0.0,
            revive: player.revive_progress,
            x: player.pos.x,
            z: player.pos.z,
        })
        .collect()
}

fn active_gun(player: &Player) -> (u32, u32) {
    let gun = if player.downed { &player.sidearm } else { &player.primary };
    (gun.ammo, gun.mag_size)
}

#[derive(Resource, Debug)]
pub struct ActNow {
    cues: Vec<String>,
    seeded: bool,
    low_hp_armed: bool,
    low_hp_next: f32,
    ammo_low: bool,
    ammo_downed: bool,
}

impl Default for ActNow {
    fn default() -> Self {
        Self {
            cues: Vec::new(),
            seeded: false,
            low_hp_armed: true,
            low_hp_next: 0.0,
            ammo_low: false,
            ammo_downed: false,
        }
    }
}

impl ActNow {
    pub fn cues(&self) -> &[String] {
        &self.cues
    }
}

/// Turns queued cues into flat UI voices. `flush_ui_cues` plays them with no world position.
pub fn dispatch(mut commands: Commands, mut act: ResMut<ActNow>) {
    for id in act.cues.drain(..) {
        commands.spawn(UiRequest(id));
    }
}

pub fn note(
    state: &mut ActNow,
    warnings: Warnings,
    ability: PlayedAbility,
    revive_range: f32,
    local_token: u64,
    interact: bool,
    before: &[Seat],
    after: &[Seat],
    now: f32,
    elapsed: f32,
) {
    let Some(local_before) = before.iter().find(|seat| seat.token == local_token) else {
        return;
    };
    let Some(local_after) = after.iter().find(|seat| seat.token == local_token) else {
        return;
    };
    if !state.seeded {
        state.ammo_low = ammo_is_low(local_before.ammo, local_before.mag, warnings.ammo_low_fraction);
        state.ammo_downed = local_before.downed;
        state.low_hp_armed = true;
        state.seeded = true;
    }
    if low_hp_due(state, local_after, now, warnings) {
        state.cues.push(LOW_HP_WARN.to_string());
    }
    if ammo_crossed(state, local_after, ability, warnings.ammo_low_fraction) {
        state.cues.push(AMMO_LOW_WARN.to_string());
    }
    if local_after.alive && ability_just_ready(local_before.ability_cd, local_after.ability_cd, ability.cooldown, elapsed) {
        state.cues.push(ABILITY_READY.to_string());
    }
    for seat in after {
        let prev = before.iter().find(|other| other.token == seat.token).map(|other| other.revive).unwrap_or(seat.revive);
        if !(prev <= 0.0 && seat.revive > 0.0) {
            continue;
        }
        let local_is_target = seat.token == local_token;
        let reviver = local_after.alive
            && !local_after.downed
            && interact
            && seat.token != local_token
            && horiz(local_after.x, local_after.z, seat.x, seat.z) <= revive_range;
        if local_is_target || reviver {
            state.cues.push(REVIVE_START.to_string());
        }
    }
}

fn low_hp_due(state: &mut ActNow, seat: &Seat, now: f32, warnings: Warnings) -> bool {
    if !seat.alive || !(seat.max_hp > 0.0) {
        return false;
    }
    let frac = seat.hp / seat.max_hp;
    if frac >= warnings.low_hp_rearm_fraction {
        state.low_hp_armed = true;
    }
    if seat.downed || frac > warnings.low_hp_fraction {
        return false;
    }
    if state.low_hp_armed || now + 1e-4 >= state.low_hp_next {
        state.low_hp_armed = false;
        state.low_hp_next = now + warnings.low_hp_repeat_s;
        return true;
    }
    false
}

/// Magazine threshold is `ceil(mag * fraction)`. Silent while this ability's
/// infinite-magazine flag is set and the ability duration is still running.
fn ammo_crossed(state: &mut ActNow, seat: &Seat, ability: PlayedAbility, fraction: f32) -> bool {
    if seat.downed != state.ammo_downed {
        state.ammo_downed = seat.downed;
        state.ammo_low = ammo_is_low(seat.ammo, seat.mag, fraction);
        return false;
    }
    let low = ammo_is_low(seat.ammo, seat.mag, fraction);
    let suppressed = ability.infinite_magazine && seat.ability_active;
    let play = low && !state.ammo_low && !suppressed && seat.alive;
    state.ammo_low = low;
    play
}

pub fn ammo_is_low(ammo: u32, mag: u32, fraction: f32) -> bool {
    if mag == 0 {
        return false;
    }
    let threshold = (mag as f32 * fraction).ceil() as u32;
    ammo <= threshold
}

/// The clock hit zero by ticking out this hero's own cooldown.
/// A same-step refund clears a remainder larger than the time that passed.
pub fn ability_just_ready(before: f32, after: f32, cooldown: f32, elapsed: f32) -> bool {
    if !(before > 0.0 && after == 0.0) {
        return false;
    }
    if !(cooldown > 0.0) || before > cooldown + 1e-3 {
        return false;
    }
    before <= elapsed + 1e-3
}

fn horiz(ax: f32, az: f32, bx: f32, bz: f32) -> f32 {
    let dx = ax - bx;
    let dz = az - bz;
    (dx * dx + dz * dz).sqrt()
}

#[cfg(test)]
mod tests {
    use gemini_sim::content::Content;
    use gemini_sim::SIM_DT;

    use super::*;

    fn warnings() -> Warnings {
        Content::embedded().warnings
    }

    fn seat(token: u64) -> Seat {
        Seat {
            token,
            hp: 100.0,
            max_hp: 100.0,
            downed: false,
            alive: true,
            ammo: 36,
            mag: 36,
            ability_cd: 0.0,
            ability_active: false,
            revive: 0.0,
            x: 0.0,
            z: 0.0,
        }
    }

    fn ability(cooldown: f32, infinite_magazine: bool) -> PlayedAbility {
        PlayedAbility { cooldown, infinite_magazine }
    }

    #[test]
    fn low_ammo_reads_the_flag_and_rounds_up() {
        let content = Content::embedded();
        assert!((content.warnings.ammo_low_fraction - 0.25).abs() < 1e-4);
        assert_eq!(ammo_is_low(9, 36, 0.25), true);
        assert_eq!(ammo_is_low(10, 36, 0.25), false);
        assert_eq!(ammo_is_low(1, 5, 0.25), true);
        let vex = content.ability_of(&content.hero.id).unwrap();
        assert!(vex.infinite_magazine);
        assert!((vex.cooldown - 20.0).abs() < 1e-4);
        for card in &content.heroes {
            let row = content.ability_of(&card.id).unwrap();
            assert_eq!(row.infinite_magazine, card.id == content.hero.id);
            assert!(row.cooldown > 0.0);
        }
        let mut state = ActNow::default();
        let mut before = seat(1);
        before.ammo = 10;
        let mut after = before.clone();
        after.ammo = 9;
        after.ability_active = true;
        note(&mut state, warnings(), vex, 2.4, 1, false, &[before.clone()], &[after.clone()], 0.0, SIM_DT);
        assert!(state.cues().is_empty(), "infinite magazine stays quiet {:?}", state.cues());
        let mut quiet = ActNow::default();
        note(&mut quiet, warnings(), ability(vex.cooldown, false), 2.4, 1, false, &[before], &[after], 0.0, SIM_DT);
        assert_eq!(quiet.cues(), &[AMMO_LOW_WARN.to_string()]);
    }

    #[test]
    fn ability_ready_uses_that_heros_cooldown_and_skips_a_refund() {
        let content = Content::embedded();
        let nyra = content.ability_of("nyra").unwrap();
        let kip = content.ability_of("kip").unwrap();
        assert!((nyra.cooldown - 24.0).abs() < 1e-4);
        assert!((kip.cooldown - 16.0).abs() < 1e-4);
        assert!(!nyra.infinite_magazine && !kip.infinite_magazine);
        assert!(ability_just_ready(SIM_DT, 0.0, kip.cooldown, SIM_DT));
        assert!(ability_just_ready(SIM_DT, 0.0, nyra.cooldown, SIM_DT));
        assert!(!ability_just_ready(0.5, 0.0, nyra.cooldown, SIM_DT), "refund remainder is larger than one step");
        assert!(!ability_just_ready(nyra.cooldown + 1.0, 0.0, kip.cooldown, nyra.cooldown + 1.0));
        assert!(!ability_just_ready(0.0, 0.0, kip.cooldown, SIM_DT));
        let mut state = ActNow::default();
        let mut before = seat(1);
        before.ability_cd = SIM_DT;
        let mut after = before.clone();
        after.ability_cd = 0.0;
        note(&mut state, warnings(), kip, 2.4, 1, false, &[before], &[after], SIM_DT, SIM_DT);
        assert_eq!(state.cues(), &[ABILITY_READY.to_string()]);
    }

    #[test]
    fn low_hp_repeats_until_rearm_and_stays_quiet_while_downed() {
        let warn = warnings();
        let mut state = ActNow::default();
        let mut high = seat(1);
        let mut low = high.clone();
        low.hp = 30.0;
        note(&mut state, warn, ability(20.0, false), 2.4, 1, false, &[high.clone()], &[low.clone()], 1.0, SIM_DT);
        assert_eq!(state.cues(), &[LOW_HP_WARN.to_string()]);
        state.cues.clear();
        note(&mut state, warn, ability(20.0, false), 2.4, 1, false, &[low.clone()], &[low.clone()], 1.5, SIM_DT);
        assert!(state.cues().is_empty());
        note(&mut state, warn, ability(20.0, false), 2.4, 1, false, &[low.clone()], &[low.clone()], 1.0 + warn.low_hp_repeat_s, SIM_DT);
        assert_eq!(state.cues(), &[LOW_HP_WARN.to_string()]);
        state.cues.clear();
        let mut downed = low.clone();
        downed.downed = true;
        downed.hp = 10.0;
        note(&mut state, warn, ability(20.0, false), 2.4, 1, false, &[low.clone()], &[downed.clone()], 10.0, SIM_DT);
        assert!(state.cues().is_empty(), "downed {:?}", state.cues());
        high.hp = 40.0;
        note(&mut state, warn, ability(20.0, false), 2.4, 1, false, &[downed], &[high.clone()], 11.0, SIM_DT);
        assert!(state.cues().is_empty());
        low.hp = 29.0;
        note(&mut state, warn, ability(20.0, false), 2.4, 1, false, &[high], &[low], 11.1, SIM_DT);
        assert_eq!(state.cues(), &[LOW_HP_WARN.to_string()]);
    }

    #[test]
    fn revive_start_is_only_the_reviver_and_the_downed_player() {
        let range = Content::embedded().combat.revive_range;
        let mut downed = seat(2);
        downed.downed = true;
        downed.x = 1.0;
        let mut started = downed.clone();
        started.revive = 0.1;
        let mut local = seat(1);
        local.x = 1.0;
        let mut state = ActNow::default();
        note(&mut state, warnings(), ability(20.0, true), range, 1, true, &[local.clone(), downed.clone()], &[local.clone(), started.clone()], 1.0, SIM_DT);
        assert_eq!(state.cues(), &[REVIVE_START.to_string()]);
        let mut far = ActNow::default();
        local.x = 100.0;
        note(&mut far, warnings(), ability(20.0, true), range, 1, true, &[local.clone(), downed.clone()], &[local.clone(), started.clone()], 1.0, SIM_DT);
        assert!(far.cues().is_empty());
        let mut heard = ActNow::default();
        note(&mut heard, warnings(), ability(20.0, false), range, 2, false, &[local, downed], &[local_at(1), started], 1.0, SIM_DT);
        assert_eq!(heard.cues(), &[REVIVE_START.to_string()]);
    }

    fn local_at(token: u64) -> Seat {
        seat(token)
    }
}
