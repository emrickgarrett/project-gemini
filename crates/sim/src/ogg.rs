//! Read a Vorbis loop length from an Ogg file. Bar lines use this, not bpm.

/// Decoded length in seconds: last granule position / sample rate.
/// Returns `None` if the bytes are not an Ogg Vorbis stream.
pub fn vorbis_seconds(data: &[u8]) -> Option<f32> {
    let mut sample_rate: Option<u32> = None;
    let mut last_granule: i64 = 0;
    let mut i = 0;
    while i + 27 <= data.len() {
        if &data[i..i + 4] != b"OggS" {
            return None;
        }
        let granule = i64::from_le_bytes(data[i + 6..i + 14].try_into().ok()?);
        let segments = data[i + 26] as usize;
        if i + 27 + segments > data.len() {
            return None;
        }
        let table = &data[i + 27..i + 27 + segments];
        let body_len: usize = table.iter().map(|s| *s as usize).sum();
        let body_at = i + 27 + segments;
        if body_at + body_len > data.len() {
            return None;
        }
        if sample_rate.is_none() && body_len >= 16 {
            let body = &data[body_at..body_at + body_len];
            if body[0] == 1 && body.len() >= 16 && &body[1..7] == b"vorbis" {
                sample_rate = Some(u32::from_le_bytes(body[12..16].try_into().ok()?));
            }
        }
        if granule > last_granule {
            last_granule = granule;
        }
        i = body_at + body_len;
    }
    let rate = sample_rate? as f32;
    if rate <= 0.0 || last_granule <= 0 {
        return None;
    }
    Some(last_granule as f32 / rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(granule: i64, body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"OggS");
        out.push(0);
        out.push(0);
        out.extend_from_slice(&granule.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.push(1);
        out.push(body.len() as u8);
        out.extend_from_slice(body);
        out
    }

    #[test]
    fn reads_sample_rate_and_granule() {
        let mut id = vec![1];
        id.extend_from_slice(b"vorbis");
        id.extend_from_slice(&0u32.to_le_bytes());
        id.push(2);
        id.extend_from_slice(&48_000u32.to_le_bytes());
        id.resize(64, 0);
        let mut bytes = page(0, &id);
        bytes.extend(page(96_000, &[0, 1, 2, 3]));
        let seconds = vorbis_seconds(&bytes).unwrap();
        assert!((seconds - 2.0).abs() < 1e-4, "{seconds}");
    }
}
