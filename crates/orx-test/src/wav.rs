//! WAV files: writes 32-bit float, reads 32-bit float and 16/24-bit PCM.

use std::fs;
use std::io;
use std::path::Path;

/// Audio of a WAV file: one vector per channel
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f32>>,
}

impl Audio {
    pub fn frames(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }
}

/// Encode channels (same length) as a 32-bit float WAV file
pub fn encode(channels: &[&[f32]], sample_rate: u32) -> Vec<u8> {
    let n = channels.len().max(1) as u16;
    let frames = channels.first().map_or(0, |c| c.len());
    assert!(channels.iter().all(|c| c.len() == frames), "channels differ in length");
    let data_len = frames * n as usize * 4;
    let mut out = Vec::with_capacity(58 + data_len);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((50 + data_len) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    // fmt: WAVE_FORMAT_IEEE_FLOAT, with the cbSize field
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&18u32.to_le_bytes());
    out.extend_from_slice(&3u16.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * u32::from(n) * 4).to_le_bytes());
    out.extend_from_slice(&(n * 4).to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    // fact chunk, required for non-PCM formats
    out.extend_from_slice(b"fact");
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&(frames as u32).to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data_len as u32).to_le_bytes());
    for i in 0..frames {
        for c in channels {
            out.extend_from_slice(&c[i].to_le_bytes());
        }
    }
    out
}

/// Write channels as a 32-bit float WAV file
pub fn write(path: impl AsRef<Path>, channels: &[&[f32]], sample_rate: u32) -> io::Result<()> {
    if let Some(dir) = path.as_ref().parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, encode(channels, sample_rate))
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// Decode a WAV file (float 32, PCM 16 or 24, WAVE_FORMAT_EXTENSIBLE too)
pub fn decode(bytes: &[u8]) -> io::Result<Audio> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(bad("not a RIFF/WAVE file"));
    }
    let u16_at = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
    let u32_at = |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    let mut pos = 12;
    let mut format = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let len = u32_at(bytes, pos + 4) as usize;
        let body = bytes.get(pos + 8..pos + 8 + len).ok_or_else(|| bad("truncated chunk"))?;
        if id == b"fmt " {
            if body.len() < 16 {
                return Err(bad("short fmt chunk"));
            }
            let mut tag = u16_at(body, 0);
            if tag == 0xfffe && body.len() >= 26 {
                tag = u16_at(body, 24);
            }
            format = Some((tag, u16_at(body, 2), u32_at(body, 4), u16_at(body, 14)));
        } else if id == b"data" {
            let (tag, n, rate, bits) = format.ok_or_else(|| bad("data before fmt"))?;
            let n = usize::from(n.max(1));
            let width = usize::from(bits / 8);
            let frames = body.len() / (width * n).max(1);
            let mut channels = vec![Vec::with_capacity(frames); n];
            for f in 0..frames {
                for (c, ch) in channels.iter_mut().enumerate() {
                    let i = (f * n + c) * width;
                    let v = match (tag, bits) {
                        (3, 32) => f32::from_le_bytes([body[i], body[i + 1], body[i + 2], body[i + 3]]),
                        (1, 16) => f32::from(i16::from_le_bytes([body[i], body[i + 1]])) / 32768.0,
                        (1, 24) => {
                            let v = i32::from_le_bytes([0, body[i], body[i + 1], body[i + 2]]) >> 8;
                            v as f32 / 8_388_608.0
                        }
                        _ => return Err(bad("unsupported sample format")),
                    };
                    ch.push(v);
                }
            }
            return Ok(Audio { sample_rate: rate, channels });
        }
        pos += 8 + len + (len & 1);
    }
    Err(bad("no data chunk"))
}

pub fn read(path: impl AsRef<Path>) -> io::Result<Audio> {
    decode(&fs::read(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_round_trip() {
        let l = [0.0, 0.5, -1.0, 1e-20];
        let r = [1.0, -0.25, 0.125, f32::MIN_POSITIVE];
        let audio = decode(&encode(&[&l, &r], 44_100)).unwrap();
        assert_eq!(audio.sample_rate, 44_100);
        assert_eq!(audio.channels, vec![l.to_vec(), r.to_vec()]);
    }
}
