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

    let reserved = voices.len();
    let room = audio.voice_cap.saturating_sub(reserved);
    voices.extend(rest.into_iter().take(room));
    // Critical cues play even when that pushes the mix past the cap.
    voices.extend(critical);
    voices
}

fn score_of(audio: &AudioTable, def: &SfxDef, pos: Vec3, listener: Vec3) -> f32 {
    let dx = pos.x - listener.x;
    let dy = pos.y - listener.y;
    let dz = pos.z - listener.z;
    let dist = (dx * dx + dy * dy + dz * dz).sqrt().max(audio.min_distance);
    def.loudness / dist
}
