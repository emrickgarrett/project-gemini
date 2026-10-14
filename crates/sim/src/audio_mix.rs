//! Voice picker. Playback stays in the client; this only decides who is heard.

use crate::content::{AudioTable, SfxDef};
use crate::math::Vec3;

#[derive(Clone, Debug)]
pub struct SoundEvent {
    pub id: String,
    pub pos: Vec3,
    /// Bug that owns the cue. 0 is the player or a global sting.
    pub owner: u32,
    /// When set, this replaces the row priority. `None` uses the mix row.
    pub priority: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Voice {
    pub id: String,
    pub score: f32,
    pub critical: bool,
    pub crowd: bool,
    pub gain: f32,
    pub pos: Vec3,
    pub priority: i32,
    pub is_warning: bool,
}

/// A voice that is already sounding. Warnings use this for nearest-wins.
/// One-shots use it so `max_instances` counts what is already in the air.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveVoice {
    pub id: String,
    pub pos: Vec3,
    /// Content priority this voice was played at. Teammate steps use `footstep.remote_priority`.
    pub priority: i32,
    /// A fade still holds its slot until it ends. A warning holds its row slot.
    /// A stolen one-shot or stealable prop loop holds its place in the voice cap.
    pub fading: bool,
}

/// New voices to start, and voices already playing that should fade out.
#[derive(Clone, Debug, PartialEq)]
pub struct VoicePick {
    pub voices: Vec<Voice>,
    pub fade: Vec<LiveVoice>,
    /// Warning voices that will be sounding after this pick, including fades.
    /// A newcomer waiting on a fade is not a slot.
    pub warning_slots: usize,
    /// Slots still open after this pick. Stolen loops resume into these.
    pub free: usize,
}

/// `waiting` holds each warning newcomer that has won its row and is waiting for a
/// fade to free the slot. The same vec is the state for the next frame: a nearer
/// candidate for that row replaces the voice that was waiting.
pub fn select_voices(
    audio: &AudioTable,
    events: &[SoundEvent],
    listener: Vec3,
    playing: &[LiveVoice],
    waiting: &mut Vec<Voice>,
) -> VoicePick {
    let mut crowd_counts: Vec<(String, u32, f32, Vec3)> = Vec::new();
    let mut shots: Vec<Voice> = Vec::new();

    for event in events {
        let def = audio
            .sfx
            .iter()
            .find(|s| s.id == event.id)
            .unwrap_or_else(|| panic!("voice {} is not in the mix", event.id));
        if def.crowd {
            let dx = event.pos.x - listener.x;
            let dz = event.pos.z - listener.z;
            let dist = (dx * dx + dz * dz).sqrt();
            if dist > audio.crowd_radius {
                continue;
            }
            if let Some(slot) = crowd_counts.iter_mut().find(|c| c.0 == def.id) {
                slot.1 += 1;
                if dist < slot.2 {
                    slot.2 = dist;
                    slot.3 = event.pos;
                }
            } else {
                crowd_counts.push((def.id.clone(), 1, dist, event.pos));
            }
        } else {
            let score = score_of(audio, def, event.pos, listener);
            shots.push(Voice {
                id: def.id.clone(),
                score,
                critical: def.critical,
                crowd: false,
                gain: def.loudness,
                pos: event.pos,
                priority: event.priority.unwrap_or(def.priority),
                is_warning: def.is_warning,
            });
        }
    }

    let mut voices = Vec::new();
    for (id, count, _dist, pos) in crowd_counts {
        let def = audio.sfx.iter().find(|s| s.id == id).expect("crowd def");
        let fullness = if audio.crowd_full_count == 0 {
            1.0
        } else {
            (count as f32 / audio.crowd_full_count as f32).clamp(0.0, 1.0)
        };
        voices.push(Voice {
            id,
            score: def.loudness * fullness,
            critical: false,
            crowd: true,
            gain: def.loudness * fullness,
            pos,
            priority: def.priority,
            is_warning: def.is_warning,
        });
    }

    shots.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| b.score.total_cmp(&a.score))
            .then_with(|| a.id.cmp(&b.id))
    });
    // Outside the cap: they play, they do not take a slot, and they are not a steal.
    let (outside, sorted): (Vec<Voice>, Vec<Voice>) = shots.into_iter().partition(|voice| {
        voice_def(audio, &voice.id).outside_voice_cap
    });
    let (warnings, mut fade, warning_slots) = admit_warnings(audio, &sorted, playing, listener, waiting);
    // The crowd loop keeps playing when no swarmer is in range, so the slot is
    // held every frame. One-shots cannot fill it and overflow when the swarm arrives.
    let heard_crowd = voices.iter().filter(|voice| voice.crowd).count();
    let crowd_slots = heard_crowd.max(reserved_crowd_slots(audio));
    let (warnings, warning_slots) = fit_warnings(
        audio,
        listener,
        playing,
        crowd_slots,
        warnings,
        &mut fade,
        waiting,
        warning_slots,
    );
    let others: Vec<Voice> = sorted.iter().filter(|voice| !voice.is_warning).cloned().collect();
    let capped = cap_instances(audio, others, playing);
    let mut step_count = playing.iter().filter(|voice| is_player_step(audio, &voice.id)).count() as u32;
    let rest: Vec<Voice> = capped
        .into_iter()
        .filter(|voice| {
            if !is_player_step(audio, &voice.id) {
                return true;
            }
            if step_count >= audio.footstep.max_voices {
                return false;
            }
            step_count += 1;
            true
        })
        .collect();
    let (continuing, rest): (Vec<Voice>, Vec<Voice>) = rest.into_iter().partition(|voice| {
        continues_live_loop(audio, voice, playing, &fade)
    });
    voices.extend(warnings);
    // Loops already sounding are kept here and are not taken out of `room` again.
    voices.extend(continuing);
    // One-shots already sounding are not in `voices`. Warnings are `warning_slots`.
    // A warning that did not fit is waiting on a fade, so it is not in either count.
    // Stealable prop loops already sounding are in `live`, including while they fade.
    let live = playing.iter().filter(|voice| occupies_voice(audio, voice)).count();
    let used = crowd_slots + warning_slots + live;
    let room = audio.voice_cap.saturating_sub(used);
    let fresh: Vec<Voice> = rest.into_iter().take(room).collect();
    let taken = fresh.len();
    voices.extend(fresh);
    // Critical cues that the cap dropped still play. Warning rows stay on their own cap,
    // so a farther warning is not put back. Ones already kept are not doubled.
    let mut claimed: Vec<(String, Vec3)> = voices.iter().map(|voice| (voice.id.clone(), voice.pos)).collect();
    let mut over = 0usize;
    for crit in sorted.into_iter().filter(|voice| voice.critical && !voice.is_warning) {
        if let Some(index) = claimed.iter().position(|(id, pos)| {
            id == &crit.id && pos.x == crit.pos.x && pos.y == crit.pos.y && pos.z == crit.pos.z
        }) {
            claimed.remove(index);
        } else {
            voices.push(crit);
            over += 1;
        }
    }
    // `max_instances` still applies. These cues do not enter `room` and do not fade a victim.
    voices.extend(cap_instances(audio, outside, playing));
    VoicePick {
        voices,
        fade,
        warning_slots,
        free: room.saturating_sub(taken + over),
    }
}

/// Nearest-wins inside one warning row. A row never reads another row's slots.
fn admit_warnings(
    audio: &AudioTable,
    shots: &[Voice],
    playing: &[LiveVoice],
    listener: Vec3,
    waiting: &mut Vec<Voice>,
) -> (Vec<Voice>, Vec<LiveVoice>, usize) {
    let mut fresh: std::collections::BTreeMap<String, Vec<Voice>> = std::collections::BTreeMap::new();
    for voice in shots.iter().filter(|voice| voice.is_warning) {
        let row = row_id(audio, &voice.id);
        fresh.entry(row).or_default().push(voice.clone());
    }
    let mut pending: std::collections::BTreeMap<String, Vec<Voice>> = std::collections::BTreeMap::new();
    for mut voice in std::mem::take(waiting) {
        let def = audio
            .sfx
            .iter()
            .find(|row| row.id == voice.id)
            .unwrap_or_else(|| panic!("waiting voice {} is not in the mix", voice.id));
        if !def.is_warning {
            panic!("waiting voice {} is not a warning", voice.id);
        }
        voice.score = score_of(audio, def, voice.pos, listener);
        pending.entry(def.row_id.clone()).or_default().push(voice);
    }
    let mut live: std::collections::BTreeMap<String, Vec<(LiveVoice, f32, bool)>> = std::collections::BTreeMap::new();
    for voice in playing {
        let def = audio
            .sfx
            .iter()
            .find(|row| row.id == voice.id)
            .unwrap_or_else(|| panic!("playing voice {} is not in the mix", voice.id));
        if !def.is_warning {
            continue;
        }
        let score = score_of(audio, def, voice.pos, listener);
        live.entry(def.row_id.clone())
            .or_default()
            .push((voice.clone(), score, voice.fading));
    }
    let mut rows: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    rows.extend(fresh.keys().cloned());
    rows.extend(live.keys().cloned());
    rows.extend(pending.keys().cloned());
    let mut play = Vec::new();
    let mut fade = Vec::new();
    let mut next_wait = Vec::new();
    let mut slots = 0usize;
    for row in rows {
        let def = audio
            .sfx
            .iter()
            .find(|item| item.id == row)
            .unwrap_or_else(|| panic!("warning row {row} is missing"));
        let cap = def.max_instances;
        if cap == 0 {
            panic!("warning row {row} max_instances must be > 0");
        }
        let mut occupied = live.remove(&row).unwrap_or_default();
        let mut events = fresh.remove(&row).unwrap_or_default();
        if let Some(held) = pending.remove(&row) {
            events.extend(held);
        }
        events.sort_by(|a, b| {
            b.score
                .total_cmp(&a.score)
                .then_with(|| a.id.cmp(&b.id))
                .then_with(|| a.pos.x.total_cmp(&b.pos.x))
                .then_with(|| a.pos.y.total_cmp(&b.pos.y))
                .then_with(|| a.pos.z.total_cmp(&b.pos.z))
        });
        // The nearest voice that won the row and did not start. One fade frees one slot.
        let mut replacement: Option<Voice> = None;
        for voice in events {
            if (occupied.len() as u32) < cap {
                occupied.push((
                    LiveVoice {
                        id: voice.id.clone(),
                        pos: voice.pos,
                        priority: voice.priority,
                        fading: false,
                    },
                    voice.score,
                    false,
                ));
                play.push(voice);
                continue;
            }
            // A fade still holds its slot. The newcomer waits; it does not start on top.
            if occupied.iter().any(|item| item.2) {
                if replacement.is_none() {
                    replacement = Some(voice);
                }
                continue;
            }
            let far_at = occupied
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| a.1.total_cmp(&b.1).then_with(|| a.0.pos.x.total_cmp(&b.0.pos.x)))
                .map(|(index, _)| index)
                .expect("a full warning row has a farthest voice");
            if voice.score.total_cmp(&occupied[far_at].1) != std::cmp::Ordering::Greater {
                continue;
            }
            let (mut victim, victim_score, _) = occupied.remove(far_at);
            victim.fading = true;
            fade.push(victim.clone());
            occupied.push((victim, victim_score, true));
            replacement = Some(voice);
        }
        if let Some(voice) = replacement {
            next_wait.push(voice);
        }
        slots += occupied.len();
    }
    *waiting = next_wait;
    (play, fade, slots)
}

/// Warnings that fit in the voice cap start. The rest take a live stealable voice
/// and wait out its fade, the same way a replacement newcomer waits on its row.
/// A stealable prop loop is a victim. The bed and the crowd loop are not.
/// The victim order is the content priority, then the farthest voice, then the id.
fn fit_warnings(
    audio: &AudioTable,
    listener: Vec3,
    playing: &[LiveVoice],
    crowd_slots: usize,
    warnings: Vec<Voice>,
    fade: &mut Vec<LiveVoice>,
    waiting: &mut Vec<Voice>,
    warning_slots: usize,
) -> (Vec<Voice>, usize) {
    if warnings.len() > warning_slots {
        panic!("started warnings exceed warning slots");
    }
    let live = playing.iter().filter(|voice| occupies_voice(audio, voice)).count();
    let already = warning_slots - warnings.len();
    let mut used = crowd_slots + already + live;
    let mut reserved = playing
        .iter()
        .filter(|voice| is_stealable_voice(audio, voice) && voice.fading)
        .count();
    let mut victims = steal_order(audio, listener, playing);
    let mut started = Vec::new();
    let mut slots = already;
    for warning in warnings {
        if used < audio.voice_cap {
            started.push(warning);
            used += 1;
            slots += 1;
            continue;
        }
        if reserved > 0 {
            reserved -= 1;
            waiting.push(warning);
            continue;
        }
        let Some(index) = victims.iter().position(|candidate| !candidate.taken) else {
            panic!("warning {} has no stealable one-shot to take", warning.id);
        };
        let mut victim = victims[index].voice.clone();
        victims[index].taken = true;
        victim.fading = true;
        fade.push(victim);
        waiting.push(warning);
    }
    (started, slots)
}

struct StealCandidate {
    voice: LiveVoice,
    distance: f32,
    taken: bool,
}

fn steal_order(audio: &AudioTable, listener: Vec3, playing: &[LiveVoice]) -> Vec<StealCandidate> {
    let mut victims: Vec<StealCandidate> = playing
        .iter()
        .filter(|voice| is_stealable_voice(audio, voice) && !voice.fading)
        .map(|voice| StealCandidate {
            distance: distance(voice.pos, listener),
            voice: voice.clone(),
            taken: false,
        })
        .collect();
    victims.sort_by(|a, b| {
        a.voice
            .priority
            .cmp(&b.voice.priority)
            .then_with(|| b.distance.total_cmp(&a.distance))
            .then_with(|| a.voice.id.cmp(&b.voice.id))
            .then_with(|| a.voice.pos.x.total_cmp(&b.voice.pos.x))
            .then_with(|| a.voice.pos.y.total_cmp(&b.voice.pos.y))
            .then_with(|| a.voice.pos.z.total_cmp(&b.voice.pos.z))
    });
    victims
}

fn distance(pos: Vec3, listener: Vec3) -> f32 {
    let dx = pos.x - listener.x;
    let dy = pos.y - listener.y;
    let dz = pos.z - listener.z;
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn row_id(audio: &AudioTable, id: &str) -> String {
    audio
        .sfx
        .iter()
        .find(|row| row.id == id)
        .unwrap_or_else(|| panic!("mix row {id} is missing"))
        .row_id
        .clone()
}

fn budget_voices(sfx: &[SfxDef], keep: impl Fn(&SfxDef) -> bool) -> Result<u32, String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut sum = 0u32;
    for row in sfx {
        if !keep(row) || !seen.insert(row.row_id.clone()) {
            continue;
        }
        let canonical = sfx.iter().find(|item| item.id == row.row_id).ok_or_else(|| {
            format!("alias {} names missing source row {}", row.id, row.row_id)
        })?;
        sum += canonical.max_instances;
    }
    Ok(sum)
}

/// One-shots and stealable loops hold a voice-cap slot. Warnings are counted on
/// their own. The bed, the crowd loop, any other non-stealable loop, and
/// `outside_voice_cap` rows do not.
fn occupies_voice(audio: &AudioTable, voice: &LiveVoice) -> bool {
    let def = voice_def(audio, &voice.id);
    if def.outside_voice_cap || def.is_warning {
        return false;
    }
    if def.looped {
        return def.stealable;
    }
    true
}

fn is_stealable_voice(audio: &AudioTable, voice: &LiveVoice) -> bool {
    let def = voice_def(audio, &voice.id);
    def.stealable && !def.is_warning && !def.outside_voice_cap
}

fn continues_live_loop(audio: &AudioTable, voice: &Voice, playing: &[LiveVoice], fading: &[LiveVoice]) -> bool {
    let def = voice_def(audio, &voice.id);
    if !(def.stealable && def.looped && !def.is_warning) {
        return false;
    }
    if fading.iter().any(|gone| gone.id == voice.id && gone.pos == voice.pos) {
        return false;
    }
    playing
        .iter()
        .any(|live| !live.fading && live.id == voice.id && live.pos == voice.pos)
}

fn voice_def<'a>(audio: &'a AudioTable, id: &str) -> &'a SfxDef {
    audio
        .sfx
        .iter()
        .find(|row| row.id == id)
        .unwrap_or_else(|| panic!("playing voice {id} is not in the mix"))
}

/// Crowd-loop instances the mix budget always holds, even when no swarmer is near.
fn reserved_crowd_slots(audio: &AudioTable) -> usize {
    match budget_voices(&audio.sfx, |row| row.crowd && row.looped) {
        Ok(slots) => slots as usize,
        Err(err) => panic!("{err}"),
    }
}

fn is_player_step(audio: &AudioTable, id: &str) -> bool {
    audio
        .footstep
        .rows
        .values()
        .any(|ids| ids.iter().any(|row| row == id))
}

fn cap_instances(audio: &AudioTable, voices: Vec<Voice>, playing: &[LiveVoice]) -> Vec<Voice> {
    let mut counts: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    for live in playing {
        let def = audio
            .sfx
            .iter()
            .find(|row| row.id == live.id)
            .unwrap_or_else(|| panic!("playing voice {} is not in the mix", live.id));
        if def.is_warning || def.looped {
            continue;
        }
        *counts.entry(live.id.clone()).or_insert(0) += 1;
    }
    let mut kept = Vec::new();
    for voice in voices {
        let cap = audio
            .sfx
            .iter()
            .find(|s| s.id == voice.id)
            .unwrap_or_else(|| panic!("voice {} is not in the mix", voice.id))
            .max_instances;
        let n = counts.entry(voice.id.clone()).or_insert(0);
        if cap > 0 && *n >= cap {
            continue;
        }
        *n += 1;
        kept.push(voice);
    }
    kept
}

/// Linear gain from a dB row. `10^(db/20)`.
pub fn db_to_linear(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

pub fn load_mix(text: &str) -> Result<AudioTable, String> {
    let root: toml::Value = toml::from_str(text).map_err(|err| err.to_string())?;
    let table = root.as_table().ok_or("audio mix is not a table")?;
    let number = |key: &str| -> Result<f64, String> {
        let value = table.get(key).ok_or_else(|| format!("audio mix is missing {key}"))?;
        value
            .as_float()
            .or_else(|| value.as_integer().map(|i| i as f64))
            .ok_or_else(|| format!("audio mix {key} is not a number"))
    };
    let voice_cap = table
        .get("voice_cap")
        .and_then(|v| v.as_integer())
        .ok_or("audio mix is missing voice_cap")? as usize;
    let critical_rows = table
        .get("critical")
        .and_then(|v| v.as_array())
        .ok_or("audio mix is missing critical")?;
    let critical: Vec<String> = critical_rows
        .iter()
        .map(|v| v.as_str().map(|s| s.to_string()).ok_or_else(|| "critical entry is not a string".to_string()))
        .collect::<Result<_, _>>()?;
    let alias = table.get("alias").and_then(|v| v.as_table()).ok_or("audio mix is missing alias")?;
    let mut by_file: std::collections::BTreeMap<String, SfxDef> = std::collections::BTreeMap::new();
    let mut outside_flag: std::collections::BTreeMap<String, bool> = std::collections::BTreeMap::new();
    for (key, value) in table {
        if matches!(
            key.as_str(),
            "voice_cap"
                | "hit_reserve"
                | "warning_fade"
                | "loop_resume_fade"
                | "crowd_radius"
                | "crowd_full_count"
                | "min_distance"
                | "critical"
                | "outside_voice_cap"
                | "alias"
                | "sting"
                | "footstep"
        ) {
            continue;
        }
        let row = value.as_table().ok_or_else(|| format!("audio mix {key} is not a row"))?;
        let db = row_number(row, key, "volume_db")?;
        let semis = row_number(row, key, "pitch_rand_semitones")?;
        let max_instances = row
            .get("max_instances")
            .and_then(|v| v.as_integer())
            .ok_or_else(|| format!("{key} is missing max_instances"))? as u32;
        let looped = row
            .get("loop")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| format!("{key} is missing loop"))?;
        let category = row
            .get("category")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("{key} is missing category"))?;
        let stealable = match row.get("stealable") {
            None => return Err(format!("{key} is missing stealable")),
            Some(value) => value
                .as_bool()
                .ok_or_else(|| format!("{key} stealable is not a bool"))?,
        };
        let flat = match row.get("flat") {
            None => return Err(format!("{key} is missing flat")),
            Some(value) => value.as_bool().ok_or_else(|| format!("{key} flat is not a bool"))?,
        };
        let is_warning = match row.get("is_warning") {
            Some(value) => value
                .as_bool()
                .ok_or_else(|| format!("{key} is_warning is not a bool"))?,
            None => false,
        };
        if is_warning && stealable {
            return Err(format!("{key} is a warning and must set stealable = false"));
        }
        if is_warning && max_instances == 0 {
            return Err(format!("{key} is a warning and max_instances must be > 0"));
        }
        let volume = db_to_linear(db);
        let sync_at = if row.contains_key("sync_at") {
            Some(row_number(row, key, "sync_at")?)
        } else {
            None
        };
        let path = match row.get("path") {
            Some(value) => {
                let path = value.as_str().filter(|path| !path.is_empty()).ok_or_else(|| format!("{key} path is empty"))?;
                path.to_string()
            }
            // Rows that omit path live at audio/sfx/{key}.ogg. A path key is used as written.
            None => format!("audio/sfx/{key}.ogg"),
        };
        let priority = match row.get("priority") {
            None => return Err(format!("{key} is missing priority")),
            Some(value) => value
                .as_integer()
                .map(|n| n as i32)
                .or_else(|| value.as_float().map(|n| n as i32))
                .ok_or_else(|| format!("{key} priority is not a number"))?,
        };
        if let Some(value) = row.get("outside_voice_cap") {
            let flag = value
                .as_bool()
                .ok_or_else(|| format!("{key} outside_voice_cap is not a bool"))?;
            outside_flag.insert(key.clone(), flag);
        }
        by_file.insert(
            key.clone(),
            SfxDef {
                id: key.clone(),
                row_id: key.clone(),
                path,
                loudness: volume,
                volume,
                crowd: category == "swarm_bed",
                critical: critical.iter().any(|id| id == key),
                max_instances,
                looped,
                pitch_semitones: semis,
                sync_at,
                category_ui: category == "ui",
                stealable,
                is_warning,
                flat,
                priority,
                category: category.to_string(),
                outside_voice_cap: false,
            },
        );
    }
    let mut sfx: Vec<SfxDef> = by_file.values().cloned().collect();
    for (event, target) in alias {
        let name = target.as_str().ok_or_else(|| format!("alias {event} is not a string"))?;
        let src = by_file
            .get(name)
            .ok_or_else(|| format!("alias {event} names missing file {name}"))?;
        let mut copy = src.clone();
        copy.id = event.clone();
        copy.critical = copy.critical || critical.iter().any(|id| id == event);
        sfx.push(copy);
    }
    let mut sting = Vec::new();
    if let Some(rows) = table.get("sting").and_then(|v| v.as_table()) {
        for (id, value) in rows {
            let row = value.as_table().ok_or_else(|| format!("sting {id} is not a row"))?;
            let path = row
                .get("path")
                .and_then(|v| v.as_str())
                .filter(|path| !path.is_empty())
                .ok_or_else(|| format!("sting {id} is missing path"))?;
            let label = format!("sting {id}");
            let volume = db_to_linear(row_number(row, &label, "volume_db")?);
            sting.push(crate::content::StingDef {
                id: id.clone(),
                path: path.to_string(),
                volume,
            });
        }
    }
    let footstep = match table.get("footstep") {
        None => crate::content::Footsteps::default(),
        Some(value) => parse_footsteps(value)?,
    };
    let hit_reserve = match table.get("hit_reserve") {
        None => return Err("audio mix is missing hit_reserve".into()),
        Some(value) => value
            .as_integer()
            .ok_or_else(|| "hit_reserve is not an integer".to_string())?,
    };
    if hit_reserve < 0 {
        return Err("hit_reserve must be >= 0".into());
    }
    let hit_reserve = hit_reserve as usize;
    let warning_fade = number("warning_fade")? as f32;
    if !(warning_fade > 0.0) {
        return Err("warning_fade must be > 0".into());
    }
    let outside_rows = match table.get("outside_voice_cap") {
        None => return Err("audio mix is missing outside_voice_cap".into()),
        Some(value) => value
            .as_array()
            .ok_or_else(|| "outside_voice_cap is not a list".to_string())?,
    };
    let mut outside_ids = Vec::new();
    for value in outside_rows {
        let id = value
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| "outside_voice_cap entry is not a string".to_string())?;
        if outside_ids.iter().any(|seen| seen == id) {
            return Err(format!("outside_voice_cap repeats {id}"));
        }
        if !sfx.iter().any(|row| row.id == id) {
            return Err(format!("outside_voice_cap names missing row {id}"));
        }
        outside_ids.push(id.to_string());
    }
    for row in &mut sfx {
        let listed = outside_ids.iter().any(|id| id == &row.id || id == &row.row_id);
        let flagged = outside_flag.get(&row.row_id).copied();
        row.outside_voice_cap = match (listed, flagged) {
            (true, Some(true)) => true,
            (true, Some(false)) => {
                return Err(format!(
                    "{} is listed in outside_voice_cap and must set outside_voice_cap = true",
                    row.row_id
                ));
            }
            (true, None) => {
                return Err(format!(
                    "{} is listed in outside_voice_cap and is missing outside_voice_cap",
                    row.row_id
                ));
            }
            (false, Some(true)) => {
                return Err(format!(
                    "{} sets outside_voice_cap but is not listed in outside_voice_cap",
                    row.row_id
                ));
            }
            (false, Some(false) | None) => false,
        };
    }
    let loop_resume_fade = number("loop_resume_fade")? as f32;
    if !(loop_resume_fade > 0.0) {
        return Err("loop_resume_fade must be > 0".into());
    }
    let warning_voices = budget_voices(&sfx, |row| row.is_warning)?;
    let crowd_loops = budget_voices(&sfx, |row| row.crowd && row.looped)?;
    let used = warning_voices as usize + crowd_loops as usize + hit_reserve;
    if used > voice_cap {
        return Err(format!(
            "warning voices {warning_voices} + crowd loops {crowd_loops} + hit_reserve {hit_reserve} exceed voice_cap {voice_cap}"
        ));
    }
    // Stealable prop loops stay out of this sum. A warning can take them while they play.
    let blocked = budget_voices(&sfx, |row| {
        row.is_warning || (row.crowd && row.looped) || (!row.is_warning && !row.looped && !row.stealable)
    })?;
    if blocked as usize >= voice_cap {
        return Err(format!(
            "non-stealable voices {blocked} fill voice_cap {voice_cap}, so a warning has no stealable one-shot to take"
        ));
    }
    let mut emitted: Vec<String> = code_voice_ids().iter().map(|id| (*id).to_string()).collect();
    for ids in footstep.rows.values() {
        emitted.extend(ids.iter().cloned());
    }
    let table = AudioTable {
        voice_cap,
        hit_reserve,
        warning_fade,
        loop_resume_fade,
        crowd_radius: number("crowd_radius")? as f32,
        crowd_full_count: table
            .get("crowd_full_count")
            .and_then(|v| v.as_integer())
            .ok_or("audio mix is missing crowd_full_count")? as u32,
        min_distance: number("min_distance")? as f32,
        sfx,
        sting,
        pitch: Vec::new(),
        footstep,
    };
    require_emitted_voices(&table, emitted)?;
    Ok(table)
}

/// Ids the sim and the client pass to the mixer as literals. Content-driven ids are
/// checked with [`require_emitted_voices`] once those tables have loaded.
fn code_voice_ids() -> &'static [&'static str] {
    &[
        "dash",
        "jump",
        "fuel_regen_paused",
        "wave",
        "hit",
        "hurt",
        "downed",
        "pickup",
        "vent_warn",
        "vent_burst",
        "perch_climb_warn",
        "swarmer_crowd",
        "beacon_pulse",
        "hive_nest_idle",
        "relay_tower_hum",
        "arena_01_bed",
        "revive_start",
        "low_hp_warn",
        "ammo_low_warn",
        "ability_ready",
        "host_away",
        "host_back",
    ]
}

/// Every id the game can put on a voice must already be a mix row.
pub fn require_emitted_voices(
    audio: &AudioTable,
    ids: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<(), String> {
    for id in ids {
        let id = id.as_ref();
        if id.is_empty() {
            return Err("emitted voice id is empty".into());
        }
        if !audio.sfx.iter().any(|row| row.id == id) {
            return Err(format!("voice {id} is missing from the mix"));
        }
    }
    Ok(())
}

fn parse_footsteps(value: &toml::Value) -> Result<crate::content::Footsteps, String> {
    let row = value.as_table().ok_or_else(|| "footstep is not a table".to_string())?;
    let stride = row_number(row, "footstep", "stride")?;
    let min_speed = row_number(row, "footstep", "min_speed")?;
    if !(stride > 0.0) {
        return Err("footstep stride must be > 0".into());
    }
    if !(min_speed > 0.0) {
        return Err("footstep min_speed must be > 0".into());
    }
    let remote_priority = row
        .get("remote_priority")
        .and_then(|value| value.as_integer())
        .ok_or_else(|| "footstep is missing remote_priority".to_string())? as i32;
    let max_voices = row
        .get("max_voices")
        .and_then(|value| value.as_integer())
        .ok_or_else(|| "footstep is missing max_voices".to_string())? as u32;
    if max_voices == 0 {
        return Err("footstep max_voices must be > 0".into());
    }
    let rows_table = row
        .get("rows")
        .and_then(|value| value.as_table())
        .ok_or_else(|| "footstep is missing rows".to_string())?;
    let mut rows = std::collections::BTreeMap::new();
    for (kind, list) in rows_table {
        let items = list.as_array().ok_or_else(|| format!("footstep.rows {kind} must be an array"))?;
        if items.is_empty() {
            return Err(format!("footstep.rows {kind} is empty"));
        }
        let mut ids = Vec::new();
        for (index, item) in items.iter().enumerate() {
            let id = item
                .as_str()
                .filter(|text| !text.is_empty())
                .ok_or_else(|| format!("footstep.rows {kind}[{index}] must be a string"))?;
            ids.push(id.to_string());
        }
        rows.insert(kind.clone(), ids);
    }
    Ok(crate::content::Footsteps {
        stride,
        min_speed,
        remote_priority,
        max_voices,
        rows,
    })
}

fn row_number(row: &toml::Table, id: &str, key: &str) -> Result<f32, String> {
    let value = row.get(key).ok_or_else(|| format!("{id} is missing {key}"))?;
    value
        .as_float()
        .or_else(|| value.as_integer().map(|n| n as f64))
        .map(|n| n as f32)
        .ok_or_else(|| format!("{id} {key} is not a number"))
}

#[cfg(test)]
mod tests {
    use super::load_mix;

    #[test]
    fn warnings_stay_unstealable_and_prop_loops_stay_stealable() {
        let audio = load_mix(include_str!("../../../content/audio_mix.toml")).expect("audio mix");
        assert_eq!(audio.voice_cap, 32);
        assert!((audio.loop_resume_fade - 0.3).abs() < 1e-6);
        let mut warnings = 0;
        for row in &audio.sfx {
            if row.is_warning {
                assert!(!row.stealable, "{}", row.id);
                warnings += 1;
            }
            if row.looped && row.category != "prop" {
                assert!(!row.stealable, "{}", row.id);
            }
        }
        assert!(warnings >= 11, "warning rows");
        for id in ["hive_nest_idle", "relay_tower_hum"] {
            let row = audio.sfx.iter().find(|row| row.id == id).unwrap_or_else(|| panic!("missing {id}"));
            assert!(row.looped && row.stealable && row.category == "prop", "{id}");
        }
        let bed = audio.sfx.iter().find(|row| row.id == "arena_01_bed").expect("arena_01_bed");
        assert!(bed.looped && !bed.stealable && bed.category == "music");
        let revive = audio.sfx.iter().find(|row| row.id == "revive_complete").expect("revive_complete");
        assert!(revive.stealable);
        assert!(!revive.is_warning);
        assert!(!revive.looped);
        let blocked = super::budget_voices(&audio.sfx, |row| {
            row.is_warning || (row.crowd && row.looped) || (!row.is_warning && !row.looped && !row.stealable)
        })
        .expect("blocked");
        let props = super::budget_voices(&audio.sfx, |row| row.looped && row.stealable).expect("prop loops");
        assert!(props > 0, "stealable loops");
        assert!((blocked as usize) < audio.voice_cap, "blocked {blocked}");
        assert!(
            blocked as usize + props as usize >= audio.voice_cap,
            "stealable loops stay out of the non-stealable load budget"
        );
    }

    #[test]
    fn a_warning_without_stealable_false_fails_to_load() {
        let text = r#"
voice_cap = 8
crowd_radius = 1.0
crowd_full_count = 1
min_distance = 1.0
critical = []

[alias]

[beep]
volume_db = 0
pitch_rand_semitones = 0
max_instances = 1
loop = false
category = "ui"
is_warning = true
"#;
        let err = load_mix(text).expect_err("warning defaults to stealable");
        assert!(err.contains("beep"), "{err}");
        assert!(err.contains("stealable"), "{err}");
    }

    #[test]
    fn a_row_missing_stealable_names_itself() {
        let text = include_str!("../../../content/audio_mix.toml").replacen("stealable = true\n", "", 1);
        let err = load_mix(&text).expect_err("missing stealable");
        assert!(err.contains("acid_impact"), "{err}");
        assert!(err.contains("missing stealable"), "{err}");
    }

    #[test]
    fn act_now_rows_are_flat_player_warnings() {
        let audio = load_mix(include_str!("../../../content/audio_mix.toml")).expect("audio mix");
        let rows = [
            ("revive_start", -4.0, 2u32),
            ("low_hp_warn", -5.0, 1),
            ("ammo_low_warn", -8.0, 1),
            ("ability_ready", -3.0, 1),
        ];
        for (id, db, cap) in rows {
            let row = audio.sfx.iter().find(|row| row.id == id).unwrap_or_else(|| panic!("missing {id}"));
            assert!((row.volume - super::db_to_linear(db)).abs() < 1e-4, "{id}");
            assert_eq!(row.max_instances, cap, "{id}");
            assert!(row.is_warning, "{id}");
            assert!(!row.stealable, "{id}");
            assert!(row.flat, "{id}");
            assert!(!row.category_ui, "{id}");
            assert!(!row.looped, "{id}");
            assert!(row.pitch_semitones.abs() < 1e-6, "{id}");
        }
        let alias = include_str!("../../../content/audio_mix.toml");
        assert!(alias.contains("hit = \"acid_impact\""));
    }

    #[test]
    fn a_bad_path_does_not_fall_back_to_the_sfx_folder() {
        let text = r#"
voice_cap = 8
crowd_radius = 1.0
crowd_full_count = 1
min_distance = 1.0
critical = []

[alias]

[beep]
path = ""
volume_db = 0
pitch_rand_semitones = 0
max_instances = 1
loop = false
category = "ui"
stealable = false
flat = true
"#;
        let err = load_mix(text).expect_err("empty path");
        assert!(err.contains("beep"), "{err}");
        assert!(err.contains("path"), "{err}");
    }

    /// Native only. Wasm has no assets directory beside the crate.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn every_mix_row_names_a_file_under_assets() {
        let audio = load_mix(include_str!("../../../content/audio_mix.toml")).expect("audio mix");
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets");
        let mut ids = Vec::new();
        for row in audio.sfx.iter() {
            ids.push(row.id.as_str());
            let file = root.join(&row.path);
            assert!(file.is_file(), "{} -> {}", row.id, file.display());
        }
        for row in audio.sting.iter() {
            ids.push(row.id.as_str());
            let file = root.join(&row.path);
            assert!(file.is_file(), "sting {} -> {}", row.id, file.display());
        }
        for id in ["crate_common_hit", "crate_rare_hit", "crate_epic_hit", "crate_legendary_hit"] {
            assert!(ids.contains(&id), "{id} is not a mix row");
            let row = audio.sfx.iter().find(|row| row.id == id).unwrap();
            assert_eq!(row.path, format!("audio/stings/{id}.ogg"));
            assert_ne!(row.path, format!("audio/sfx/{id}.ogg"));
        }
    }

    #[test]
    fn worst_case_keeps_warnings_inside_the_voice_cap() {
        let audio = load_mix(include_str!("../../../content/audio_mix.toml")).expect("audio mix");
        let warn_min = audio
            .sfx
            .iter()
            .filter(|row| row.is_warning)
            .map(|row| row.priority)
            .min()
            .expect("warnings");
        let hits = ["acid_impact", "weakpoint_hit", "player_hurt"];
        let hit_max = hits
            .iter()
            .map(|id| audio.sfx.iter().find(|row| row.id == *id).unwrap().priority)
            .max()
            .unwrap();
        let own_max = audio
            .sfx
            .iter()
            .filter(|row| row.id.starts_with("step_"))
            .map(|row| row.priority)
            .max()
            .unwrap();
        let prop_max = ["hive_nest_idle", "relay_tower_hum", "beacon_pulse", "arena_01_bed"]
            .iter()
            .map(|id| audio.sfx.iter().find(|row| row.id == *id).unwrap().priority)
            .max()
            .unwrap();
        let remote = audio.footstep.remote_priority;
        assert_eq!(audio.footstep.max_voices, 6);
        assert!(warn_min > hit_max, "warnings {warn_min} hits {hit_max}");
        assert!(hit_max > remote, "hits {hit_max} remote {remote}");
        assert!(remote > own_max, "remote {remote} steps {own_max}");
        assert!(own_max > prop_max, "steps {own_max} props {prop_max}");
        let pulse = audio.sfx.iter().find(|row| row.id == "beacon_pulse").unwrap();
        assert!((pulse.volume - super::db_to_linear(-16.0)).abs() < 1e-4);

        let mut events = Vec::new();
        let mut n = 0u32;
        let push = |events: &mut Vec<super::SoundEvent>, id: &str, count: u32, priority: Option<i32>, n: &mut u32| {
            for _ in 0..count {
                let i = *n;
                *n += 1;
                events.push(super::SoundEvent {
                    id: id.to_string(),
                    pos: crate::math::Vec3::new(i as f32, 0.0, 0.0),
                    owner: i,
                    priority,
                });
            }
        };
        for row in audio.sfx.iter().filter(|row| row.is_warning) {
            push(&mut events, &row.id, 1, None, &mut n);
        }
        for id in hits {
            let row = audio.sfx.iter().find(|row| row.id == id).unwrap();
            push(&mut events, id, row.max_instances.max(1), None, &mut n);
        }
        let mut wave = 0u32;
        for row in audio.sfx.iter().filter(|row| {
            !row.is_warning && !row.critical && row.priority == 70 && row.category == "enemy"
        }) {
            push(&mut events, &row.id, row.max_instances.max(1), None, &mut n);
            wave += 1;
        }
        assert!(wave > 0, "a full wave needs non-warning enemy sounds at priority 70");
        let step = audio.footstep.rows.values().next().unwrap()[0].clone();
        push(&mut events, &step, audio.footstep.max_voices + 2, Some(remote), &mut n);
        for id in audio.sfx.iter().filter(|row| row.id.starts_with("step_")) {
            push(&mut events, &id.id, 1, None, &mut n);
        }
        for id in ["hive_nest_idle", "relay_tower_hum", "beacon_pulse"] {
            let row = audio.sfx.iter().find(|row| row.id == id).unwrap();
            push(&mut events, id, row.max_instances.max(1), None, &mut n);
        }
        let voices = super::select_voices(&audio, &events, crate::math::Vec3::ZERO, &[], &mut Vec::new()).voices;
        assert!(voices.len() <= audio.voice_cap, "voices {}", voices.len());
        for row in audio.sfx.iter().filter(|row| row.is_warning) {
            assert!(voices.iter().any(|voice| voice.id == row.id), "stolen {}", row.id);
        }
        let step_ids: Vec<&str> = audio.footstep.rows.values().flatten().map(|id| id.as_str()).collect();
        let steps = voices.iter().filter(|voice| step_ids.contains(&voice.id.as_str())).count();
        assert!(steps <= audio.footstep.max_voices as usize, "steps {steps}");
        let quiet: Vec<_> = events
            .iter()
            .filter(|event| !step_ids.contains(&event.id.as_str()))
            .cloned()
            .collect();
        let without = super::select_voices(&audio, &quiet, crate::math::Vec3::ZERO, &[], &mut Vec::new()).voices;
        for voice in without.iter().filter(|voice| voice.priority == 70) {
            assert!(
                voices.iter().any(|kept| kept.id == voice.id && kept.pos == voice.pos),
                "step pushed out {} at priority 70",
                voice.id
            );
        }
    }

    #[test]
    fn footstep_max_voices_is_required() {
        let text = include_str!("../../../content/audio_mix.toml").replace("max_voices = 6\n", "");
        let err = load_mix(&text).expect_err("missing max_voices");
        assert!(err.contains("max_voices"), "{err}");
    }

    #[test]
    fn every_warning_at_its_cap_leaves_hit_reserve() {
        let audio = load_mix(include_str!("../../../content/audio_mix.toml")).expect("audio mix");
        let rows: Vec<_> = audio
            .sfx
            .iter()
            .filter(|row| row.is_warning && row.id == row.row_id)
            .collect();
        let mut events = Vec::new();
        let mut n = 0u32;
        for row in &rows {
            for instance in 0..row.max_instances {
                events.push(super::SoundEvent {
                    id: row.id.clone(),
                    pos: crate::math::Vec3::new(audio.min_distance * (n as f32 + 2.0), instance as f32, 0.0),
                    owner: n,
                    priority: None,
                });
                n += 1;
            }
        }
        let pick = super::select_voices(&audio, &events, crate::math::Vec3::ZERO, &[], &mut Vec::new());
        assert!(pick.fade.is_empty());
        let warning_sum: u32 = rows.iter().map(|row| row.max_instances).sum();
        let heard = pick.voices.iter().filter(|voice| voice.is_warning).count();
        assert_eq!(heard, warning_sum as usize);
        for row in &rows {
            let count = pick.voices.iter().filter(|voice| voice.id == row.id).count();
            assert_eq!(count, row.max_instances as usize, "{}", row.id);
        }
        let crowd = super::budget_voices(&audio.sfx, |row| row.crowd && row.looped).expect("crowd loops") as usize;
        assert_eq!(audio.voice_cap - pick.voices.len(), crowd + audio.hit_reserve);
        assert_eq!(warning_sum as usize + crowd + audio.hit_reserve, audio.voice_cap);

        let nest = audio.sfx.iter().find(|row| row.id == "hive_nest_idle").expect("hive_nest_idle");
        let tower = audio.sfx.iter().find(|row| row.id == "relay_tower_hum").expect("relay_tower_hum");
        let bed = audio.sfx.iter().find(|row| row.id == "arena_01_bed").expect("arena_01_bed");
        let mut playing = Vec::new();
        for instance in 0..nest.max_instances {
            playing.push(super::LiveVoice {
                id: nest.id.clone(),
                pos: crate::math::Vec3::new((instance + 1) as f32 * 10.0, 0.0, 0.0),
                priority: nest.priority,
                fading: false,
            });
        }
        playing.push(super::LiveVoice {
            id: tower.id.clone(),
            pos: crate::math::Vec3::new(25.0, 0.0, 0.0),
            priority: tower.priority,
            fading: false,
        });
        playing.push(super::LiveVoice {
            id: bed.id.clone(),
            pos: crate::math::Vec3::new(100.0, 0.0, 0.0),
            priority: bed.priority,
            fading: false,
        });
        let loops: Vec<_> = playing.iter().filter(|voice| voice.id != bed.id).cloned().collect();
        let loop_count = loops.len();
        assert_eq!(loop_count, (nest.max_instances + tower.max_instances) as usize);
        assert!(loop_count + warning_sum as usize + crowd > audio.voice_cap);
        let mut with_loops = events.clone();
        for voice in &loops {
            with_loops.push(super::SoundEvent {
                id: voice.id.clone(),
                pos: voice.pos,
                owner: 0,
                priority: None,
            });
        }
        let mut waiting = Vec::new();
        let crowded = super::select_voices(&audio, &with_loops, crate::math::Vec3::ZERO, &playing, &mut waiting);
        let overflow = loop_count + warning_sum as usize + crowd - audio.voice_cap;
        assert_eq!(crowded.fade.len(), overflow);
        assert_eq!(waiting.len(), overflow);
        assert!(crowded.fade.iter().all(|voice| voice.id != bed.id));
        let started = crowded.voices.iter().filter(|voice| voice.is_warning).count();
        assert_eq!(started + overflow, warning_sum as usize);
        assert_eq!(loop_count + started + crowd, audio.voice_cap);
        let mut expected = loops.clone();
        expected.sort_by(|a, b| {
            a.priority
                .cmp(&b.priority)
                .then_with(|| b.pos.x.total_cmp(&a.pos.x))
                .then_with(|| a.id.cmp(&b.id))
                .then_with(|| a.pos.x.total_cmp(&b.pos.x))
        });
        for (voice, want) in crowded.fade.iter().zip(expected.iter()) {
            assert_eq!(voice.id, want.id);
            assert_eq!(voice.pos, want.pos);
            assert_eq!(voice.priority, want.priority);
        }
        for voice in &loops {
            let stolen = crowded.fade.iter().any(|faded| faded.id == voice.id && faded.pos == voice.pos);
            let kept = crowded.voices.iter().any(|kept| kept.id == voice.id && kept.pos == voice.pos);
            assert_eq!(stolen, !kept, "{}", voice.id);
        }
    }

    #[test]
    fn a_quiet_frame_reserves_the_crowd_slot_for_the_swarm() {
        let audio = load_mix(include_str!("../../../content/audio_mix.toml")).expect("audio mix");
        let crowd = super::budget_voices(&audio.sfx, |row| row.crowd && row.looped).expect("crowd loops") as usize;
        assert!(crowd > 0);
        let steps: Vec<&str> = audio.footstep.rows.values().flatten().map(|id| id.as_str()).collect();
        let mut events = Vec::new();
        let mut n = 0u32;
        for row in audio.sfx.iter().filter(|row| {
            row.id == row.row_id
                && !row.is_warning
                && !row.looped
                && !row.critical
                && !row.crowd
                && row.max_instances > 0
                && !steps.contains(&row.id.as_str())
        }) {
            for instance in 0..row.max_instances {
                events.push(super::SoundEvent {
                    id: row.id.clone(),
                    pos: crate::math::Vec3::new(n as f32 + 1.0, instance as f32, 0.0),
                    owner: n,
                    priority: None,
                });
                n += 1;
                if events.len() > audio.voice_cap {
                    break;
                }
            }
            if events.len() > audio.voice_cap {
                break;
            }
        }
        assert!(events.len() > audio.voice_cap, "need more one-shots than the cap");
        let quiet = super::select_voices(&audio, &events, crate::math::Vec3::ZERO, &[], &mut Vec::new());
        assert!(quiet.voices.iter().all(|voice| !voice.crowd));
        let oneshots = quiet.voices.len();
        assert_eq!(oneshots + crowd, audio.voice_cap);
        let playing: Vec<super::LiveVoice> = quiet
            .voices
            .iter()
            .map(|voice| super::LiveVoice {
                id: voice.id.clone(),
                pos: voice.pos,
                priority: voice.priority,
                fading: false,
            })
            .collect();
        let swarm = super::SoundEvent {
            id: "swarmer_crowd".into(),
            pos: crate::math::Vec3::new(0.0, 0.0, 1.0),
            owner: 0,
            priority: None,
        };
        let loud = super::select_voices(&audio, &[swarm], crate::math::Vec3::ZERO, &playing, &mut Vec::new());
        assert_eq!(loud.voices.iter().filter(|voice| voice.crowd).count(), crowd);
        assert!(loud.voices.iter().all(|voice| voice.crowd));
        assert!(loud.fade.is_empty());
        let total = playing.len() + loud.voices.len();
        assert!(total <= audio.voice_cap);
        assert_eq!(total, audio.voice_cap);
    }

    #[test]
    fn a_farther_critical_warning_stays_inside_its_row() {
        let audio = load_mix(include_str!("../../../content/audio_mix.toml")).expect("audio mix");
        let row = audio.sfx.iter().find(|row| row.id == "spitter_charge").expect("spitter_charge");
        assert!(row.critical && row.is_warning, "spitter_charge");
        let playing: Vec<super::LiveVoice> = (0..row.max_instances)
            .map(|i| super::LiveVoice {
                id: row.id.clone(),
                pos: crate::math::Vec3::new(audio.min_distance * (i as f32 + 2.0), 0.0, 0.0),
                priority: row.priority,
                fading: false,
            })
            .collect();
        let farther = super::SoundEvent {
            id: row.id.clone(),
            pos: crate::math::Vec3::new(audio.min_distance * (row.max_instances as f32 + 3.0), 0.0, 0.0),
            owner: 0,
            priority: None,
        };
        let pick = super::select_voices(&audio, &[farther], crate::math::Vec3::ZERO, &playing, &mut Vec::new());
        assert!(pick.voices.iter().all(|voice| voice.id != row.id));
        assert!(pick.fade.is_empty());
    }

    #[test]
    fn warning_budget_over_the_voice_cap_fails_to_load() {
        let audio = load_mix(include_str!("../../../content/audio_mix.toml")).expect("audio mix");
        let vent = audio.sfx.iter().find(|row| row.id == "vent_warn").expect("vent_warn");
        let from = format!(
            "[vent_warn]\nvolume_db = -3.2\npitch_rand_semitones = 0\nmax_instances = {}\n",
            vent.max_instances
        );
        let to = format!(
            "[vent_warn]\nvolume_db = -3.2\npitch_rand_semitones = 0\nmax_instances = {}\n",
            vent.max_instances + 1
        );
        let text = include_str!("../../../content/audio_mix.toml").replacen(&from, &to, 1);
        assert_ne!(text, include_str!("../../../content/audio_mix.toml"));
        let err = load_mix(&text).expect_err("over budget");
        assert!(err.contains("exceed voice_cap"), "{err}");
        assert!(err.contains("hit_reserve"), "{err}");
    }

    #[test]
    fn hit_reserve_and_warning_fade_are_required() {
        let source = include_str!("../../../content/audio_mix.toml");
        let missing_reserve = source
            .lines()
            .filter(|line| !line.starts_with("hit_reserve"))
            .collect::<Vec<_>>()
            .join("\n");
        let err = load_mix(&missing_reserve).expect_err("missing hit_reserve");
        assert!(err.contains("hit_reserve"), "{err}");
        let missing_fade = source
            .lines()
            .filter(|line| !line.starts_with("warning_fade"))
            .collect::<Vec<_>>()
            .join("\n");
        let err = load_mix(&missing_fade).expect_err("missing warning_fade");
        assert!(err.contains("warning_fade"), "{err}");
        let zero_fade = source
            .lines()
            .map(|line| if line.starts_with("warning_fade") { "warning_fade = 0" } else { line })
            .collect::<Vec<_>>()
            .join("\n");
        let err = load_mix(&zero_fade).expect_err("zero warning_fade");
        assert!(err.contains("warning_fade"), "{err}");
        let missing_resume = source
            .lines()
            .filter(|line| !line.starts_with("loop_resume_fade"))
            .collect::<Vec<_>>()
            .join("\n");
        let err = load_mix(&missing_resume).expect_err("missing loop_resume_fade");
        assert!(err.contains("loop_resume_fade"), "{err}");
        let zero_resume = source
            .lines()
            .map(|line| if line.starts_with("loop_resume_fade") { "loop_resume_fade = 0" } else { line })
            .collect::<Vec<_>>()
            .join("\n");
        let err = load_mix(&zero_resume).expect_err("zero loop_resume_fade");
        assert!(err.contains("loop_resume_fade"), "{err}");
    }

    fn stub_row(id: &str, row_id: &str, max_instances: u32) -> crate::content::SfxDef {
        crate::content::SfxDef {
            id: id.into(),
            row_id: row_id.into(),
            path: String::new(),
            loudness: 1.0,
            volume: 1.0,
            crowd: false,
            critical: false,
            max_instances,
            looped: false,
            pitch_semitones: 0.0,
            sync_at: None,
            category_ui: false,
            stealable: false,
            is_warning: true,
            flat: false,
            priority: 100,
            category: "enemy".into(),
            outside_voice_cap: false,
        }
    }

    #[test]
    fn a_missing_alias_source_row_fails_the_budget() {
        let alias = stub_row("downed", "player_downed", 2);
        let err = super::budget_voices(&[alias], |row| row.is_warning).expect_err("missing source");
        assert!(err.contains("alias downed"), "{err}");
        assert!(err.contains("player_downed"), "{err}");
    }

    #[test]
    #[should_panic(expected = "voice not_a_row is not in the mix")]
    fn an_unknown_voice_is_not_left_uncapped() {
        let audio = load_mix(include_str!("../../../content/audio_mix.toml")).expect("audio mix");
        let voice = super::Voice {
            id: "not_a_row".into(),
            score: 1.0,
            critical: false,
            crowd: false,
            gain: 1.0,
            pos: crate::math::Vec3::ZERO,
            priority: 0,
            is_warning: false,
        };
        super::cap_instances(&audio, vec![voice], &[]);
    }

    #[test]
    fn a_missing_emitted_voice_fails_to_load() {
        let source = include_str!("../../../content/audio_mix.toml");
        let header = "[jump]\n";
        let start = source.find(header).expect("jump row");
        let rest = &source[start + header.len()..];
        let end = rest.find("\n[").map(|index| start + header.len() + index + 1).unwrap_or(source.len());
        let text = format!("{}{}", &source[..start], &source[end..]);
        assert_ne!(text, source);
        let err = load_mix(&text).expect_err("missing jump");
        assert!(err.contains("voice jump is missing from the mix"), "{err}");
    }

    #[test]
    fn a_cap_with_no_stealable_one_shot_fails_to_load() {
        let source = include_str!("../../../content/audio_mix.toml");
        let from = "[jump]\nvolume_db = -1.4\npitch_rand_semitones = 0.3\nmax_instances = 2\nloop = false\ncategory = \"player\"\npriority = 70\nstealable = true\n";
        let to = "[jump]\nvolume_db = -1.4\npitch_rand_semitones = 0.3\nmax_instances = 4\nloop = false\ncategory = \"player\"\npriority = 70\nstealable = false\n";
        let text = source.replacen(from, to, 1);
        assert_ne!(text, source);
        let err = load_mix(&text).expect_err("nothing to take");
        assert!(err.contains("no stealable one-shot"), "{err}");
        assert!(err.contains("voice_cap"), "{err}");
    }

    #[test]
    fn bank_cues_never_consume_a_cap_voice_and_never_steal() {
        let audio = load_mix(include_str!("../../../content/audio_mix.toml")).expect("audio mix");
        assert_eq!(audio.voice_cap, 32);
        assert_eq!(audio.hit_reserve, 3);
        let vent = audio.sfx.iter().find(|row| row.id == "vent_warn").expect("vent_warn");
        let perch = audio.sfx.iter().find(|row| row.id == "perch_climb_warn").expect("perch_climb_warn");
        assert_eq!(vent.max_instances, 3);
        assert_eq!(perch.max_instances, 4);
        let warning_sum: u32 = audio
            .sfx
            .iter()
            .filter(|row| row.is_warning && row.id == row.row_id)
            .map(|row| row.max_instances)
            .sum();
        assert_eq!(warning_sum, 28);
        for id in ["hive_nest_idle", "relay_tower_hum"] {
            let row = audio.sfx.iter().find(|row| row.id == id).unwrap_or_else(|| panic!("missing {id}"));
            assert!(row.looped && row.stealable, "{id} stays a counted stealable prop loop");
            let live = super::LiveVoice {
                id: row.id.clone(),
                pos: crate::math::Vec3::new(4.0, 0.0, 0.0),
                priority: row.priority,
                fading: false,
            };
            assert!(super::occupies_voice(&audio, &live), "{id} holds a cap voice");
            assert!(super::is_stealable_voice(&audio, &live), "{id} can be taken");
        }
        for id in ["ui_count_tick", "ui_bank_chime"] {
            let row = audio.sfx.iter().find(|row| row.id == id).unwrap_or_else(|| panic!("missing {id}"));
            assert!(row.outside_voice_cap, "{id}");
            assert_eq!(row.max_instances, 1, "{id}");
            assert!((row.volume - super::db_to_linear(-16.0)).abs() < 1e-4, "{id}");
            let live = super::LiveVoice {
                id: row.id.clone(),
                pos: crate::math::Vec3::ZERO,
                priority: row.priority,
                fading: false,
            };
            assert!(!super::occupies_voice(&audio, &live), "{id} consumes a cap voice");
            assert!(!super::is_stealable_voice(&audio, &live), "{id} can be stolen");
        }
        let filler = audio
            .sfx
            .iter()
            .find(|row| {
                row.stealable && !row.looped && !row.is_warning && !row.outside_voice_cap && row.id == row.row_id
            })
            .expect("stealable one-shot");
        let playing: Vec<super::LiveVoice> = (0..audio.voice_cap)
            .map(|index| super::LiveVoice {
                id: filler.id.clone(),
                pos: crate::math::Vec3::new(index as f32 + 1.0, 0.0, 0.0),
                priority: filler.priority,
                fading: false,
            })
            .collect();
        assert_eq!(playing.iter().filter(|voice| super::occupies_voice(&audio, voice)).count(), audio.voice_cap);
        let events = ["ui_count_tick", "ui_bank_chime"]
            .into_iter()
            .map(|id| super::SoundEvent {
                id: id.to_string(),
                pos: crate::math::Vec3::ZERO,
                owner: 0,
                priority: None,
            })
            .collect::<Vec<_>>();
        let mut waiting = Vec::new();
        let pick = super::select_voices(&audio, &events, crate::math::Vec3::ZERO, &playing, &mut waiting);
        assert!(pick.fade.is_empty(), "a bank cue does not steal a cap voice");
        assert!(waiting.is_empty());
        assert_eq!(pick.voices.iter().filter(|voice| voice.id == "ui_count_tick").count(), 1);
        assert_eq!(pick.voices.iter().filter(|voice| voice.id == "ui_bank_chime").count(), 1);
        assert!(
            pick.voices.iter().all(|voice| voice.id == "ui_count_tick" || voice.id == "ui_bank_chime"),
            "bank cues do not push a cap voice out of the pick"
        );
        let held = vec![super::LiveVoice {
            id: "ui_count_tick".into(),
            pos: crate::math::Vec3::ZERO,
            priority: 10,
            fading: false,
        }];
        let again = super::select_voices(&audio, &events, crate::math::Vec3::ZERO, &held, &mut Vec::new());
        assert!(again.voices.iter().all(|voice| voice.id != "ui_count_tick"), "max_instances still drops a second tick");
        assert_eq!(again.voices.iter().filter(|voice| voice.id == "ui_bank_chime").count(), 1);
        assert!(again.fade.is_empty());
        let mut with_tick = playing.clone();
        with_tick.push(super::LiveVoice {
            id: "ui_count_tick".into(),
            pos: crate::math::Vec3::ZERO,
            priority: 10,
            fading: false,
        });
        assert_eq!(
            with_tick.iter().filter(|voice| super::occupies_voice(&audio, voice)).count(),
            audio.voice_cap,
            "a live tick does not add a cap voice"
        );
    }

    #[test]
    fn outside_voice_cap_requires_the_list_and_the_row_flag() {
        let source = include_str!("../../../content/audio_mix.toml");
        let missing_list = source
            .lines()
            .filter(|line| !line.starts_with("outside_voice_cap = ["))
            .collect::<Vec<_>>()
            .join("\n");
        let err = load_mix(&missing_list).expect_err("missing list");
        assert!(err.contains("missing outside_voice_cap"), "{err}");
        let missing_flag = source.replacen("outside_voice_cap = true\n", "", 1);
        assert_ne!(missing_flag, source);
        let err = load_mix(&missing_flag).expect_err("missing row flag");
        assert!(err.contains("ui_count_tick"), "{err}");
        assert!(err.contains("missing outside_voice_cap"), "{err}");
        let bad_flag = source.replacen("outside_voice_cap = true\n", "outside_voice_cap = 1\n", 1);
        let err = load_mix(&bad_flag).expect_err("flag is not a bool");
        assert!(err.contains("ui_count_tick"), "{err}");
        assert!(err.contains("not a bool"), "{err}");
        let false_flag = source.replacen("outside_voice_cap = true\n", "outside_voice_cap = false\n", 1);
        let err = load_mix(&false_flag).expect_err("listed row is false");
        assert!(err.contains("ui_count_tick"), "{err}");
        assert!(err.contains("must set outside_voice_cap = true"), "{err}");
        let extra = source.replacen(
            "[jump]\nvolume_db = -1.4\n",
            "[jump]\nvolume_db = -1.4\noutside_voice_cap = true\n",
            1,
        );
        assert_ne!(extra, source);
        let err = load_mix(&extra).expect_err("unlisted row");
        assert!(err.contains("jump"), "{err}");
        assert!(err.contains("not listed"), "{err}");
    }
}

fn score_of(audio: &AudioTable, def: &SfxDef, pos: Vec3, listener: Vec3) -> f32 {
    let dx = pos.x - listener.x;
    let dy = pos.y - listener.y;
    let dz = pos.z - listener.z;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt().max(audio.min_distance);
    def.loudness / dist
}
