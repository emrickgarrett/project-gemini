use std::collections::BTreeMap;

use crate::content::{Content, StackRule, UpgradeDef};
use crate::rng::Rng;

#[derive(Clone, Debug)]
pub struct Modifiers {
    pub damage_pct: f32,
    pub fire_rate_pct: f32,
    pub move_speed_pct: f32,
    pub max_hp_flat: f32,
    pub magazine_pct: f32,
    pub reload_speed_pct: f32,
    pub pickup_radius_pct: f32,
    pub ammo_drop_chance: f32,
    pub heal_per_kill_flat: f32,
    pub crit_chance: f32,
    pub jetpack_fuel_pct: f32,
    pub weakpoint_damage_pct: f32,
    pub bounce_targets: f32,
    pub spitter_damage_reduction: f32,
    pub revive_speed_pct: f32,
    pub landing_knockback_radius_m: f32,
    pub shield_flat: f32,
    pub pierce_count: f32,
    pub ability_cooldown_reduction: f32,
    pub arc_proc_chance: f32,
    pub drone_count: f32,
    pub last_stand: bool,
    pub squad_damage_pct: f32,
    pub nova_kill_interval: f32,
    pub glass_cannon: bool,
}

impl Default for Modifiers {
    fn default() -> Self {
        Self {
            damage_pct: 0.0,
            fire_rate_pct: 0.0,
            move_speed_pct: 0.0,
            max_hp_flat: 0.0,
            magazine_pct: 0.0,
            reload_speed_pct: 0.0,
            pickup_radius_pct: 0.0,
            ammo_drop_chance: 0.0,
            heal_per_kill_flat: 0.0,
            crit_chance: 0.0,
            jetpack_fuel_pct: 0.0,
            weakpoint_damage_pct: 0.0,
            bounce_targets: 0.0,
            spitter_damage_reduction: 0.0,
            revive_speed_pct: 0.0,
            landing_knockback_radius_m: 0.0,
            shield_flat: 0.0,
            pierce_count: 0.0,
            ability_cooldown_reduction: 0.0,
            arc_proc_chance: 0.0,
            drone_count: 0.0,
            last_stand: false,
            squad_damage_pct: 0.0,
            nova_kill_interval: 0.0,
            glass_cannon: false,
        }
    }
}

pub fn stacked_value(def: &UpgradeDef, stacks: u32) -> f32 {
    if stacks == 0 {
        return 0.0;
    }
    match def.stack {
        StackRule::Linear => (def.value * stacks as f32).min(def.cap),
        StackRule::Diminishing => {
            let k = def.k;
            def.cap * (1.0 - 1.0 / (1.0 + k * stacks as f32))
        }
        StackRule::Unique => def.value.min(def.cap),
    }
}

fn secondary_value(def: &UpgradeDef, stacks: u32) -> f32 {
    let (Some(value), Some(cap)) = (def.secondary_value, def.secondary_cap) else {
        return 0.0;
    };
    if stacks == 0 {
        return 0.0;
    }
    (value * stacks as f32).min(cap)
}

pub fn eligible(def: &UpgradeDef, stacks: u32) -> bool {
    match def.stack {
        StackRule::Unique => stacks == 0,
        StackRule::Linear => {
            def.value > 0.0 && def.value * (stacks as f32) < def.cap - 1e-4
        }
        StackRule::Diminishing => true,
    }
}

pub fn collect(content: &Content, owned: &BTreeMap<String, u32>) -> Modifiers {
    let mut m = Modifiers::default();
    for def in &content.upgrades {
        let stacks = owned.get(&def.id).copied().unwrap_or(0);
        if stacks == 0 {
            continue;
        }
        let total = stacked_value(def, stacks);
        apply_stat(&mut m, &def.stat, total, def);
        if let Some(stat) = &def.secondary_stat {
            apply_stat(&mut m, stat, secondary_value(def, stacks), def);
        }
    }
    m
}

fn apply_stat(m: &mut Modifiers, stat: &str, total: f32, def: &UpgradeDef) {
    match stat {
        "fire_rate_pct" => m.fire_rate_pct += total,
        "damage_pct" => m.damage_pct += total,
        "move_speed_pct" => m.move_speed_pct += total,
        "max_hp_flat" => m.max_hp_flat += total,
        "magazine_pct" => m.magazine_pct += total,
        "reload_speed_pct" => m.reload_speed_pct += total,
        "pickup_radius_pct" => m.pickup_radius_pct += total,
        "ammo_drop_chance" => m.ammo_drop_chance += total,
        "heal_per_kill_flat" => m.heal_per_kill_flat += total,
        "crit_chance" => m.crit_chance += total,
        "jetpack_fuel_pct" => m.jetpack_fuel_pct += total,
        "weakpoint_damage_pct" => m.weakpoint_damage_pct += total,
        "bounce_targets" => m.bounce_targets += total,
        "spitter_damage_reduction" => m.spitter_damage_reduction += total,
        "revive_speed_pct" => m.revive_speed_pct += total,
        "landing_knockback_radius_m" => m.landing_knockback_radius_m += total,
        "shield_flat" => m.shield_flat += total,
        "pierce_count" => m.pierce_count += total,
        "ability_cooldown_reduction" => m.ability_cooldown_reduction += total,
        "arc_proc_chance" => m.arc_proc_chance += total,
        "drone_count" => m.drone_count += total,
        "last_stand" => m.last_stand = true,
        "squad_damage_pct" => m.squad_damage_pct = m.squad_damage_pct.max(total),
        "nova_kill_interval" => m.nova_kill_interval = total,
        "glass_cannon" => m.glass_cannon = true,
        _ => {
            let _ = def;
        }
    }
}

pub fn rarity_weight(content: &Content, rarity: &str) -> f32 {
    match rarity {
        "common" => content.combat.rarity_weights.common,
        "rare" => content.combat.rarity_weights.rare,
        "epic" => content.combat.rarity_weights.epic,
        _ => 0.0,
    }
}

/// Pick `n` distinct upgrades. Uniques already owned, and linear stats already
/// at cap, stay out of the pool.
pub fn roll_offers(
    rng: &mut Rng,
    content: &Content,
    owned: &BTreeMap<String, u32>,
    n: usize,
) -> Vec<String> {
    let mut pool: Vec<(String, f32)> = content
        .upgrades
        .iter()
        .filter(|def| eligible(def, owned.get(&def.id).copied().unwrap_or(0)))
        .map(|def| (def.id.clone(), rarity_weight(content, &def.rarity)))
        .filter(|(_, w)| *w > 0.0)
        .collect();
    let mut out = Vec::new();
    for _ in 0..n {
        if pool.is_empty() {
            break;
        }
        let total: f32 = pool.iter().map(|(_, w)| *w).sum();
        let mut needle = rng.next_f32() * total;
        let mut index = pool.len() - 1;
        for (i, (_, w)) in pool.iter().enumerate() {
            needle -= *w;
            if needle <= 0.0 {
                index = i;
                break;
            }
        }
        out.push(pool.remove(index).0);
    }
    out
}
