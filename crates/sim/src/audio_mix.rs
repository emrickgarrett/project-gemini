//! Voice picker. Playback stays in the client; this only decides who is heard.

use crate::content::{AudioTable, SfxDef};
use crate::math::Vec3;

#[derive(Clone, Debug)]
pub struct SoundEvent {
    pub id: String,
    pub pos: Vec3,
    /// Bug that owns the cue. 0 is the player or a global sting.
    pub owner: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Voice {
    pub id: String,
    pub score: f32,
    pub critical: bool,
    pub crowd: bool,
    pub gain: f32,
    pub pos: Vec3,
}

pub fn select_voices(
    audio: &AudioTable,
    events: &[SoundEvent],
    listener: Vec3,
) -> Vec<Voice> {
    let mut crowd_counts: Vec<(String, u32, f32, Vec3)> = Vec::new();
    let mut shots: Vec<Voice> = Vec::new();

    for event in events {
        let Some(def) = audio.sfx.iter().find(|s| s.id == event.id) else {
            continue;
        };
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
        });
    }

    let critical: Vec<Voice> = shots.iter().filter(|v| v.critical).cloned().collect();
    let mut rest: Vec<Voice> = shots.into_iter().filter(|v| !v.critical).collect();
    rest.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    rest = cap_instances(audio, rest);

    let reserved = voices.len();
    let room = audio.voice_cap.saturating_sub(reserved);
    voices.extend(rest.into_iter().take(room));
    // Critical cues play even when that pushes the mix past the cap.
    voices.extend(critical);
    voices
}

fn cap_instances(audio: &AudioTable, voices: Vec<Voice>) -> Vec<Voice> {
    let mut counts: std::collections::BTreeMap<String, u32> = std::collections::BTreeMap::new();
    let mut kept = Vec::new();
    for voice in voices {
        let cap = audio
            .sfx
            .iter()
            .find(|s| s.id == voice.id)
            .map(|s| s.max_instances)
            .unwrap_or(0);
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
    let num = |key: &str, fallback: f64| -> f32 {
        table.get(key).and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64))).unwrap_or(fallback) as f32
    };
    let voice_cap = table.get("voice_cap").and_then(|v| v.as_integer()).unwrap_or(32) as usize;
    let critical: Vec<String> = table
        .get("critical")
        .and_then(|v| v.as_array())
        .map(|rows| rows.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    let alias = table.get("alias").and_then(|v| v.as_table());
    let mut by_file: std::collections::BTreeMap<String, SfxDef> = std::collections::BTreeMap::new();
    for (key, value) in table {
        if matches!(
            key.as_str(),
            "voice_cap" | "crowd_radius" | "crowd_full_count" | "min_distance" | "critical" | "alias" | "sting"
        ) {
            continue;
        }
        let Some(row) = value.as_table() else { continue };
        if !row.contains_key("volume_db") {
            continue;
        }
        let db = row.get("volume_db").and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64))).unwrap_or(0.0) as f32;
        let semis = row.get("pitch_rand_semitones").and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64))).unwrap_or(0.0) as f32;
        let max_instances = row.get("max_instances").and_then(|v| v.as_integer()).unwrap_or(1) as u32;
        let looped = row.get("loop").and_then(|v| v.as_bool()).unwrap_or(false);
        let category = row.get("category").and_then(|v| v.as_str()).unwrap_or("");
        let volume = db_to_linear(db);
        by_file.insert(
            key.clone(),
            SfxDef {
                id: key.clone(),
                path: format!("audio/sfx/{key}.ogg"),
                loudness: volume.abs().max(0.05),
                volume,
                crowd: category == "swarm_bed",
                critical: critical.iter().any(|id| id == key),
                max_instances,
                looped,
                pitch_semitones: semis,
            },
        );
    }
    let mut sfx: Vec<SfxDef> = by_file.values().cloned().collect();
    if let Some(alias) = alias {
        for (event, target) in alias {
            let Some(name) = target.as_str() else { continue };
            let Some(src) = by_file.get(name) else { continue };
            let mut copy = src.clone();
            copy.id = event.clone();
            copy.critical = copy.critical || critical.iter().any(|id| id == event);
            sfx.push(copy);
        }
    }
    let mut sting = Vec::new();
    if let Some(rows) = table.get("sting").and_then(|v| v.as_table()) {
        for (id, value) in rows {
            let path = value.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                continue;
            }
            sting.push(crate::content::StingDef {
                id: id.clone(),
                path: path.to_string(),
            });
        }
    }
    Ok(AudioTable {
        voice_cap,
        crowd_radius: num("crowd_radius", 30.0),
        crowd_full_count: table.get("crowd_full_count").and_then(|v| v.as_integer()).unwrap_or(18) as u32,
        min_distance: num("min_distance", 0.5),
        sfx,
        sting,
        pitch: Vec::new(),
    })
}

fn score_of(audio: &AudioTable, def: &SfxDef, pos: Vec3, listener: Vec3) -> f32 {
    let dx = pos.x - listener.x;
    let dy = pos.y - listener.y;
    let dz = pos.z - listener.z;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt().max(audio.min_distance);
    def.loudness / dist
}
