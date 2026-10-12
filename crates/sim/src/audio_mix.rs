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
    for (key, value) in table {
        if matches!(
            key.as_str(),
            "voice_cap" | "crowd_radius" | "crowd_full_count" | "min_distance" | "critical" | "alias" | "sting"
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
        let mut stealable = match row.get("stealable") {
            Some(value) => value
                .as_bool()
                .ok_or_else(|| format!("{key} stealable is not a bool"))?,
            None => true,
        };
        let is_warning = match row.get("is_warning") {
            Some(value) => value
                .as_bool()
                .ok_or_else(|| format!("{key} is_warning is not a bool"))?,
            None => false,
        };
        if looped {
            stealable = false;
        }
        if is_warning && stealable {
            return Err(format!("{key} is a warning and must set stealable = false"));
        }
        let volume = db_to_linear(db);
        let sync_at = if row.contains_key("sync_at") {
            Some(row_number(row, key, "sync_at")?)
        } else {
            None
        };
        by_file.insert(
            key.clone(),
            SfxDef {
                id: key.clone(),
                path: format!("audio/sfx/{key}.ogg"),
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
    Ok(AudioTable {
        voice_cap,
        crowd_radius: number("crowd_radius")? as f32,
        crowd_full_count: table
            .get("crowd_full_count")
            .and_then(|v| v.as_integer())
            .ok_or("audio mix is missing crowd_full_count")? as u32,
        min_distance: number("min_distance")? as f32,
        sfx,
        sting,
        pitch: Vec::new(),
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
    fn warnings_and_loops_cannot_be_stolen() {
        let audio = load_mix(include_str!("../../../content/audio_mix.toml")).expect("audio mix");
        assert_eq!(audio.voice_cap, 32);
        let mut warnings = 0;
        for row in &audio.sfx {
            if row.looped || row.is_warning {
                assert!(!row.stealable, "{}", row.id);
            }
            if row.is_warning {
                warnings += 1;
            }
        }
        assert!(warnings >= 11, "warning rows");
        let revive = audio.sfx.iter().find(|row| row.id == "revive_complete").expect("revive_complete");
        assert!(revive.stealable);
        assert!(!revive.is_warning);
        assert!(!revive.looped);
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
        assert!(err.contains("stealable"), "{err}");
    }
}

fn score_of(audio: &AudioTable, def: &SfxDef, pos: Vec3, listener: Vec3) -> f32 {
    let dx = pos.x - listener.x;
    let dy = pos.y - listener.y;
    let dz = pos.z - listener.z;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt().max(audio.min_distance);
    def.loudness / dist
}
