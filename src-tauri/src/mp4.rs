// el mp4 lo armamos nosotros: mientras graba va en pedazos de un segundo (si cocuyo se cae el video sirve igual)
// y al parar se le pone el indice de toda la vida para que cualquier editor lo abra

use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    H264,
    Hevc,
    Av1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Sound {
    Aac(u32),
    Pcm,
}

#[derive(Clone, Debug)]
pub struct Packet {
    pub data: Arc<[u8]>,
    pub pts: i64,
    pub key: bool,
}

pub const TICKS: i64 = 10_000_000;
const VIDEO_SCALE: u32 = 90_000;
pub const AUDIO_RATE: u32 = 48_000;
const FRAGMENT: i64 = TICKS;

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn put64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(body.len() + 8);
    put32(&mut out, body.len() as u32 + 8);
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out
}

fn full(kind: &[u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
    let mut inner = Vec::with_capacity(body.len() + 4);
    put32(&mut inner, (version as u32) << 24 | flags);
    inner.extend_from_slice(body);
    bx(kind, &inner)
}

fn boxes(kind: &[u8; 4], parts: &[Vec<u8>]) -> Vec<u8> {
    bx(kind, &parts.concat())
}

pub fn nals(data: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut i = 0;
    let mut start = None;
    while i + 2 < data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            if let Some(from) = start {
                let mut end = i;
                while end > from && data[end - 1] == 0 {
                    end -= 1;
                }
                out.push(&data[from..end]);
            }
            i += 3;
            start = Some(i);
        } else {
            i += 1;
        }
    }
    if let Some(from) = start {
        if from < data.len() {
            out.push(&data[from..]);
        }
    }
    out
}

fn unescape(nal: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nal.len());
    let mut zeros = 0;
    for &b in nal {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

fn nal_type(codec: Codec, nal: &[u8]) -> u8 {
    match codec {
        Codec::H264 => nal[0] & 0x1F,
        _ => (nal[0] >> 1) & 0x3F,
    }
}

pub fn is_key(codec: Codec, data: &[u8]) -> bool {
    match codec {
        Codec::H264 => nals(data).iter().any(|n| !n.is_empty() && nal_type(codec, n) == 5),
        Codec::Hevc => nals(data).iter().any(|n| n.len() > 1 && (16..=21).contains(&nal_type(codec, n))),
        Codec::Av1 => obus(data).iter().any(|(kind, _)| *kind == 1),
    }
}

fn obus(data: &[u8]) -> Vec<(u8, &[u8])> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let header = data[i];
        let kind = (header >> 3) & 0xF;
        let extension = header & 4 != 0;
        let has_size = header & 2 != 0;
        let mut at = i + 1 + usize::from(extension);
        let size = if has_size {
            let mut value = 0usize;
            for shift in 0..8 {
                let Some(&byte) = data.get(at) else { return out };
                at += 1;
                value |= ((byte & 0x7F) as usize) << (shift * 7);
                if byte & 0x80 == 0 {
                    break;
                }
            }
            value
        } else {
            data.len().saturating_sub(at)
        };
        let end = (at + size).min(data.len());
        out.push((kind, &data[i..end]));
        i = end;
    }
    out
}

struct Bits<'a> {
    data: &'a [u8],
    at: usize,
}

impl Bits<'_> {
    fn bit(&mut self) -> u32 {
        let byte = self.data.get(self.at / 8).copied().unwrap_or(0);
        let bit = (byte >> (7 - self.at % 8)) & 1;
        self.at += 1;
        bit as u32
    }

    fn bits(&mut self, n: u32) -> u32 {
        (0..n).fold(0, |acc, _| acc << 1 | self.bit())
    }

    fn uvlc(&mut self) -> u32 {
        let mut zeros = 0;
        while self.bit() == 0 && zeros < 32 {
            zeros += 1;
        }
        if zeros >= 32 {
            return u32::MAX;
        }
        (1u32 << zeros) - 1 + self.bits(zeros)
    }
}

fn avcc(sps: &[u8], pps: &[u8]) -> Vec<u8> {
    let mut body = vec![1, sps[1], sps[2], sps[3], 0xFF, 0xE1];
    put16(&mut body, sps.len() as u16);
    body.extend_from_slice(sps);
    body.push(1);
    put16(&mut body, pps.len() as u16);
    body.extend_from_slice(pps);
    if matches!(sps[1], 100 | 110 | 122 | 144) {
        body.extend_from_slice(&[0xFC | 1, 0xF8, 0xF8, 0]);
    }
    bx(b"avcC", &body)
}

fn hvcc(vps: &[u8], sps: &[u8], pps: &[u8]) -> Vec<u8> {
    let raw = unescape(sps);
    let mut bits = Bits { data: &raw, at: 16 };
    bits.bits(4);
    let sub_layers = bits.bits(3);
    let nested = bits.bit();
    let ptl: Vec<u8> = raw.get(3..15).map(|p| p.to_vec()).unwrap_or_else(|| vec![0; 12]);
    let mut body = vec![1];
    body.extend_from_slice(&ptl[..1]);
    body.extend_from_slice(&ptl[1..5]);
    body.extend_from_slice(&ptl[5..11]);
    body.push(ptl[11]);
    put16(&mut body, 0xF000);
    body.push(0xFC);
    body.push(0xFC | 1);
    body.push(0xF8);
    body.push(0xF8);
    put16(&mut body, 0);
    body.push(((sub_layers + 1) as u8) << 3 | (nested as u8) << 2 | 3);
    body.push(3);
    for (kind, nal) in [(32u8, vps), (33, sps), (34, pps)] {
        body.push(0x80 | kind);
        put16(&mut body, 1);
        put16(&mut body, nal.len() as u16);
        body.extend_from_slice(nal);
    }
    bx(b"hvcC", &body)
}

fn av1c(sequence: &[u8]) -> Vec<u8> {
    let header = sequence[0];
    let mut at = 1 + usize::from(header & 4 != 0);
    if header & 2 != 0 {
        while at < sequence.len() && sequence[at] & 0x80 != 0 {
            at += 1;
        }
        at += 1;
    }
    let payload = sequence.get(at..).unwrap_or(&[]);
    let mut bits = Bits { data: payload, at: 0 };
    let profile = bits.bits(3);
    let still = bits.bit();
    let reduced = bits.bit();
    let (mut level, mut tier) = (0, 0);
    if reduced == 1 {
        level = bits.bits(5);
    } else {
        let timing = bits.bit();
        let mut decoder_model = 0;
        let mut delay_length = 0;
        if timing == 1 {
            bits.bits(32);
            bits.bits(32);
            if bits.bit() == 1 {
                bits.uvlc();
            }
            decoder_model = bits.bit();
            if decoder_model == 1 {
                delay_length = bits.bits(5) + 1;
                bits.bits(32);
                bits.bits(5);
                bits.bits(5);
            }
        }
        let presentation_delay = bits.bit();
        let points = bits.bits(5) + 1;
        for i in 0..points {
            bits.bits(12);
            let point_level = bits.bits(5);
            let point_tier = if point_level > 7 { bits.bit() } else { 0 };
            if i == 0 {
                level = point_level;
                tier = point_tier;
            }
            if decoder_model == 1 && bits.bit() == 1 {
                bits.bits(delay_length);
                bits.bits(delay_length);
                bits.bit();
            }
            if presentation_delay == 1 && bits.bit() == 1 {
                bits.bits(4);
            }
        }
    }
    let _ = still;
    let width_bits = bits.bits(4) + 1;
    let height_bits = bits.bits(4) + 1;
    bits.bits(width_bits);
    bits.bits(height_bits);
    if reduced == 0 && bits.bit() == 1 {
        bits.bits(4);
        bits.bits(3);
    }
    bits.bits(3);
    if reduced == 0 {
        bits.bits(4);
        let order_hint = bits.bit();
        if order_hint == 1 {
            bits.bits(2);
        }
        let force_screen = if bits.bit() == 1 { 2 } else { bits.bit() };
        if force_screen > 0 && bits.bit() == 0 {
            bits.bit();
        }
        if order_hint == 1 {
            bits.bits(3);
        }
    }
    bits.bits(3);
    let high_bitdepth = bits.bit();
    let twelve = if profile == 2 && high_bitdepth == 1 { bits.bit() } else { 0 };
    let mono = if profile == 1 { 0 } else { bits.bit() };
    let mut body = vec![0x81, (profile as u8) << 5 | level as u8];
    body.push((tier as u8) << 7 | (high_bitdepth as u8) << 6 | (twelve as u8) << 5 | (mono as u8) << 4 | 1 << 3 | 1 << 2);
    body.push(0);
    body.extend_from_slice(sequence);
    bx(b"av1C", &body)
}

#[derive(Clone)]
pub struct Video {
    pub codec: Codec,
    pub size: (u32, u32),
    config: Vec<u8>,
}

impl Video {
    pub fn from_key(codec: Codec, size: (u32, u32), data: &[u8]) -> Option<Video> {
        let config = match codec {
            Codec::H264 => {
                let units = nals(data);
                let sps = units.iter().find(|n| !n.is_empty() && nal_type(codec, n) == 7)?;
                let pps = units.iter().find(|n| !n.is_empty() && nal_type(codec, n) == 8)?;
                (sps.len() > 3).then(|| avcc(sps, pps))?
            }
            Codec::Hevc => {
                let units = nals(data);
                let find = |kind| units.iter().find(|n| n.len() > 1 && nal_type(codec, n) == kind).copied();
                let (vps, sps, pps) = (find(32)?, find(33)?, find(34)?);
                if sps.len() <= 15 {
                    return None;
                }
                hvcc(vps, sps, pps)
            }
            Codec::Av1 => obus(data).iter().find(|(kind, _)| *kind == 1).map(|(_, obu)| av1c(obu))?,
        };
        Some(Video { codec, size, config })
    }

    fn entry(&self) -> Vec<u8> {
        let mut body = vec![0; 6];
        put16(&mut body, 1);
        body.extend_from_slice(&[0; 16]);
        put16(&mut body, self.size.0 as u16);
        put16(&mut body, self.size.1 as u16);
        put32(&mut body, 0x0048_0000);
        put32(&mut body, 0x0048_0000);
        put32(&mut body, 0);
        put16(&mut body, 1);
        let mut name = [0u8; 32];
        let label = b"Cocuyo";
        name[0] = label.len() as u8;
        name[1..=label.len()].copy_from_slice(label);
        body.extend_from_slice(&name);
        put16(&mut body, 0x18);
        put16(&mut body, 0xFFFF);
        body.extend_from_slice(&self.config);
        let mut colour = b"nclx".to_vec();
        put16(&mut colour, 1);
        put16(&mut colour, 1);
        put16(&mut colour, 1);
        colour.push(0);
        body.extend_from_slice(&bx(b"colr", &colour));
        let kind = match self.codec {
            Codec::H264 => b"avc1",
            Codec::Hevc => b"hvc1",
            Codec::Av1 => b"av01",
        };
        bx(kind, &body)
    }

    pub fn sample(&self, data: &[u8]) -> Vec<u8> {
        match self.codec {
            Codec::Av1 => obus(data).into_iter().filter(|(kind, _)| *kind != 2).flat_map(|(_, obu)| obu.iter().copied()).collect(),
            codec => {
                let mut out = Vec::with_capacity(data.len() + 16);
                for nal in nals(data) {
                    if nal.is_empty() {
                        continue;
                    }
                    let kind = nal_type(codec, nal);
                    let skip = match codec {
                        Codec::H264 => matches!(kind, 7 | 8 | 9),
                        _ => matches!(kind, 32..=35),
                    };
                    if !skip {
                        put32(&mut out, nal.len() as u32);
                        out.extend_from_slice(nal);
                    }
                }
                out
            }
        }
    }
}

fn audio_entry(sound: Sound) -> Vec<u8> {
    let mut body = vec![0; 6];
    put16(&mut body, 1);
    body.extend_from_slice(&[0; 8]);
    put16(&mut body, 2);
    put16(&mut body, 16);
    put32(&mut body, 0);
    put32(&mut body, AUDIO_RATE << 16);
    match sound {
        Sound::Aac(bitrate) => {
            let specific = [0x05, 2, 0x11, 0x90];
            let mut config = vec![0x04, 13 + specific.len() as u8, 0x40, 0x15, 0, 0x18, 0];
            put32(&mut config, bitrate);
            put32(&mut config, bitrate);
            config.extend_from_slice(&specific);
            let mut descriptor = vec![0x03, 3 + config.len() as u8 + 3];
            put16(&mut descriptor, 0);
            descriptor.push(0);
            descriptor.extend_from_slice(&config);
            descriptor.extend_from_slice(&[0x06, 1, 2]);
            body.extend_from_slice(&full(b"esds", 0, 0, &descriptor));
            bx(b"mp4a", &body)
        }
        Sound::Pcm => bx(b"sowt", &body),
    }
}

struct Track {
    id: u32,
    scale: u32,
    video: Option<Video>,
    sound: Option<Sound>,
    pending: Vec<(Arc<[u8]>, u32, bool)>,
    held: Option<(Arc<[u8]>, i64, bool)>,
    decoded: u64,
    sizes: Vec<u32>,
    durations: Vec<u32>,
    keys: Vec<u32>,
    chunks: Vec<(u64, u32)>,
    nominal: u32,
    start: u64,
}

impl Track {
    fn ticks(&self, pts: i64) -> i64 {
        (pts as i128 * self.scale as i128 / TICKS as i128) as i64
    }

    fn push(&mut self, data: Arc<[u8]>, pts: i64, key: bool) {
        if let Some((held, at, was_key)) = self.held.take() {
            let span = (self.ticks(pts) - self.ticks(at)).max(1) as u32;
            self.nominal = span;
            self.pending.push((held, span, was_key));
        }
        self.held = Some((data, pts, key));
    }

    fn push_fixed(&mut self, data: Arc<[u8]>, span: u32) {
        self.pending.push((data, span, true));
    }

    fn release_held(&mut self) {
        if let Some((held, _, key)) = self.held.take() {
            self.pending.push((held, self.nominal.max(1), key));
        }
    }

    fn pending_span(&self) -> u64 {
        self.pending.iter().map(|(_, span, _)| *span as u64).sum()
    }

    fn duration(&self) -> u64 {
        self.durations.iter().map(|d| *d as u64).sum()
    }

    fn header(&self, final_index: bool, movie_scale: u32) -> Vec<u8> {
        let duration = if final_index { self.duration() } else { 0 };
        let movie_duration = duration * movie_scale as u64 / self.scale as u64;
        let mut tkhd = Vec::new();
        put32(&mut tkhd, 0);
        put32(&mut tkhd, 0);
        put32(&mut tkhd, self.id);
        put32(&mut tkhd, 0);
        put32(&mut tkhd, movie_duration as u32);
        put64(&mut tkhd, 0);
        put16(&mut tkhd, 0);
        put16(&mut tkhd, 0);
        put16(&mut tkhd, if self.sound.is_some() { 0x0100 } else { 0 });
        put16(&mut tkhd, 0);
        for v in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
            put32(&mut tkhd, v);
        }
        let (w, h) = self.video.as_ref().map_or((0, 0), |v| v.size);
        put32(&mut tkhd, w << 16);
        put32(&mut tkhd, h << 16);
        let mut mdhd = Vec::new();
        put32(&mut mdhd, 0);
        put32(&mut mdhd, 0);
        put32(&mut mdhd, self.scale);
        put32(&mut mdhd, duration as u32);
        put16(&mut mdhd, 0x55C4);
        put16(&mut mdhd, 0);
        let (handler, name): (&[u8; 4], &[u8]) = if self.video.is_some() { (b"vide", b"Video\0") } else { (b"soun", b"Sonido\0") };
        let mut hdlr = vec![0; 4];
        hdlr.extend_from_slice(handler);
        hdlr.extend_from_slice(&[0; 12]);
        hdlr.extend_from_slice(name);
        let media_header = if self.video.is_some() { full(b"vmhd", 0, 1, &[0; 8]) } else { full(b"smhd", 0, 0, &[0; 4]) };
        let dinf = boxes(b"dinf", &[full(b"dref", 0, 0, &[&1u32.to_be_bytes()[..], &full(b"url ", 0, 1, &[])].concat())]);
        let entry = match (&self.video, self.sound) {
            (Some(video), _) => video.entry(),
            (None, Some(sound)) => audio_entry(sound),
            _ => Vec::new(),
        };
        let stsd = full(b"stsd", 0, 0, &[&1u32.to_be_bytes()[..], &entry].concat());
        let stbl = if final_index { self.index(stsd) } else {
            boxes(b"stbl", &[stsd, full(b"stts", 0, 0, &[0; 4]), full(b"stsc", 0, 0, &[0; 4]), full(b"stsz", 0, 0, &[0; 8]), full(b"stco", 0, 0, &[0; 4])])
        };
        let minf = boxes(b"minf", &[media_header, dinf, stbl]);
        let mdia = boxes(b"mdia", &[full(b"mdhd", 0, 0, &mdhd), full(b"hdlr", 0, 0, &hdlr), minf]);
        let mut parts = vec![full(b"tkhd", 0, 3, &tkhd)];
        if final_index && self.start > 0 {
            let mut elst = Vec::new();
            put32(&mut elst, 2);
            put32(&mut elst, (self.start * movie_scale as u64 / self.scale as u64) as u32);
            put32(&mut elst, u32::MAX);
            put32(&mut elst, 0x0001_0000);
            put32(&mut elst, movie_duration as u32);
            put32(&mut elst, 0);
            put32(&mut elst, 0x0001_0000);
            parts.push(boxes(b"edts", &[full(b"elst", 0, 0, &elst)]));
        }
        parts.push(mdia);
        boxes(b"trak", &parts)
    }

    fn index(&self, stsd: Vec<u8>) -> Vec<u8> {
        let mut runs: Vec<(u32, u32)> = Vec::new();
        for d in &self.durations {
            match runs.last_mut() {
                Some((count, span)) if span == d => *count += 1,
                _ => runs.push((1, *d)),
            }
        }
        let mut stts = Vec::new();
        put32(&mut stts, runs.len() as u32);
        for (count, span) in runs {
            put32(&mut stts, count);
            put32(&mut stts, span);
        }
        let mut stsz = Vec::new();
        put32(&mut stsz, 0);
        put32(&mut stsz, self.sizes.len() as u32);
        for s in &self.sizes {
            put32(&mut stsz, *s);
        }
        let mut chunk_runs: Vec<(u32, u32)> = Vec::new();
        for (i, (_, count)) in self.chunks.iter().enumerate() {
            if chunk_runs.last().map(|(_, c)| c) != Some(count) {
                chunk_runs.push((i as u32 + 1, *count));
            }
        }
        let mut stsc = Vec::new();
        put32(&mut stsc, chunk_runs.len() as u32);
        for (first, count) in chunk_runs {
            put32(&mut stsc, first);
            put32(&mut stsc, count);
            put32(&mut stsc, 1);
        }
        let mut co64 = Vec::new();
        put32(&mut co64, self.chunks.len() as u32);
        for (offset, _) in &self.chunks {
            put64(&mut co64, *offset);
        }
        let mut parts = vec![stsd, full(b"stts", 0, 0, &stts)];
        if self.video.is_some() && self.keys.len() < self.sizes.len() {
            let mut stss = Vec::new();
            put32(&mut stss, self.keys.len() as u32);
            for k in &self.keys {
                put32(&mut stss, *k);
            }
            parts.push(full(b"stss", 0, 0, &stss));
        }
        parts.extend([full(b"stsc", 0, 0, &stsc), full(b"stsz", 0, 0, &stsz), full(b"co64", 0, 0, &co64)]);
        boxes(b"stbl", &parts)
    }
}

pub struct Mp4 {
    file: BufWriter<File>,
    at: u64,
    tracks: Vec<Track>,
    started: bool,
    init_at: u64,
    fragments: Vec<u64>,
    sequence: u32,
    waiting_audio: Vec<Packet>,
    codec: Codec,
    size: (u32, u32),
    fragmented: bool,
}

const MOVIE_SCALE: u32 = 1000;

impl Mp4 {
    pub fn create(path: &Path, codec: Codec, size: (u32, u32), fps: u32, sound: Option<Sound>, fragmented: bool) -> std::io::Result<Mp4> {
        let file = BufWriter::with_capacity(1 << 20, File::create(path)?);
        let mut tracks = vec![Track {
            id: 1,
            scale: VIDEO_SCALE,
            video: None,
            sound: None,
            pending: Vec::new(),
            held: None,
            decoded: 0,
            sizes: Vec::new(),
            durations: Vec::new(),
            keys: Vec::new(),
            chunks: Vec::new(),
            nominal: VIDEO_SCALE / fps.max(1),
            start: 0,
        }];
        if let Some(sound) = sound {
            tracks.push(Track {
                id: 2,
                scale: AUDIO_RATE,
                video: None,
                sound: Some(sound),
                pending: Vec::new(),
                held: None,
                decoded: 0,
                sizes: Vec::new(),
                durations: Vec::new(),
                keys: Vec::new(),
                chunks: Vec::new(),
                nominal: 1024,
                start: 0,
            });
        }
        Ok(Mp4 { file, at: 0, tracks, started: false, init_at: 0, fragments: Vec::new(), sequence: 0, waiting_audio: Vec::new(), codec, size, fragmented })
    }

    fn write(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.file.write_all(bytes)?;
        self.at += bytes.len() as u64;
        Ok(())
    }

    pub fn video(&mut self, packet: &Packet) -> std::io::Result<()> {
        if !self.started {
            if !packet.key {
                return Ok(());
            }
            let Some(video) = Video::from_key(self.codec, self.size, &packet.data) else { return Ok(()) };
            self.tracks[0].video = Some(video);
            self.start()?;
        }
        let sample: Arc<[u8]> = self.tracks[0].video.as_ref().map(|v| v.sample(&packet.data)).unwrap_or_default().into();
        self.tracks[0].push(sample, packet.pts, packet.key);
        let waiting = std::mem::take(&mut self.waiting_audio);
        for audio in waiting {
            self.audio(&audio)?;
        }
        if self.tracks[0].pending_span() >= (FRAGMENT * VIDEO_SCALE as i64 / TICKS) as u64 {
            self.flush()?;
        }
        Ok(())
    }

    pub fn audio(&mut self, packet: &Packet) -> std::io::Result<()> {
        if self.tracks.len() < 2 {
            return Ok(());
        }
        if !self.started {
            self.waiting_audio.push(packet.clone());
            return Ok(());
        }
        let track = &mut self.tracks[1];
        if packet.pts < 0 {
            return Ok(());
        }
        if track.pending.is_empty() && track.sizes.is_empty() {
            track.start = track.ticks(packet.pts).max(0) as u64;
            track.decoded = track.start;
        }
        let frames = match track.sound {
            Some(Sound::Pcm) => (packet.data.len() / 4) as u32,
            _ => 1024,
        };
        track.push_fixed(packet.data.clone(), frames);
        Ok(())
    }

    fn start(&mut self) -> std::io::Result<()> {
        let brands: &[&[u8; 4]] = &[b"isom", b"iso6", b"mp41"];
        let mut ftyp = b"isom".to_vec();
        put32(&mut ftyp, 0x200);
        for brand in brands {
            ftyp.extend_from_slice(*brand);
        }
        let ftyp = bx(b"ftyp", &ftyp);
        self.write(&ftyp)?;
        self.init_at = self.at;
        if self.fragmented {
            let moov = self.moov(false);
            self.write(&moov)?;
        }
        self.started = true;
        Ok(())
    }

    fn moov(&self, final_index: bool) -> Vec<u8> {
        let duration = if final_index {
            self.tracks.iter().map(|t| t.duration() * MOVIE_SCALE as u64 / t.scale as u64).max().unwrap_or(0)
        } else {
            0
        };
        let mut mvhd = Vec::new();
        put32(&mut mvhd, 0);
        put32(&mut mvhd, 0);
        put32(&mut mvhd, MOVIE_SCALE);
        put32(&mut mvhd, duration as u32);
        put32(&mut mvhd, 0x0001_0000);
        put16(&mut mvhd, 0x0100);
        mvhd.extend_from_slice(&[0; 10]);
        for v in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
            put32(&mut mvhd, v);
        }
        mvhd.extend_from_slice(&[0; 24]);
        put32(&mut mvhd, self.tracks.len() as u32 + 1);
        let mut parts = vec![full(b"mvhd", 0, 0, &mvhd)];
        for track in &self.tracks {
            parts.push(track.header(final_index, MOVIE_SCALE));
        }
        if !final_index {
            let trex: Vec<Vec<u8>> = self
                .tracks
                .iter()
                .map(|t| {
                    let mut body = Vec::new();
                    put32(&mut body, t.id);
                    put32(&mut body, 1);
                    put32(&mut body, 0);
                    put32(&mut body, 0);
                    put32(&mut body, 0);
                    full(b"trex", 0, 0, &body)
                })
                .collect();
            parts.push(boxes(b"mvex", &trex));
        }
        boxes(b"moov", &parts)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if !self.started || self.tracks.iter().all(|t| t.pending.is_empty()) {
            return Ok(());
        }
        self.sequence += 1;
        let mut trafs = Vec::new();
        let mut patches = Vec::new();
        let mut data_len = 0u64;
        for track in &self.tracks {
            if track.pending.is_empty() {
                continue;
            }
            let mut tfhd = Vec::new();
            put32(&mut tfhd, track.id);
            let mut tfdt = Vec::new();
            put64(&mut tfdt, track.decoded);
            let mut trun = Vec::new();
            put32(&mut trun, track.pending.len() as u32);
            let patch_in_trun = trun.len();
            put32(&mut trun, 0);
            for (data, span, key) in &track.pending {
                put32(&mut trun, *span);
                put32(&mut trun, data.len() as u32);
                put32(&mut trun, if *key { 0x0200_0000 } else { 0x0101_0000 });
            }
            let tfhd = full(b"tfhd", 0, 0x02_0000, &tfhd);
            let tfdt = full(b"tfdt", 1, 0, &tfdt);
            let before = 8 + tfhd.len() + tfdt.len() + 12 + patch_in_trun;
            let traf = boxes(b"traf", &[tfhd, tfdt, full(b"trun", 0, 0x000701, &trun)]);
            patches.push((trafs.iter().map(|t: &Vec<u8>| t.len()).sum::<usize>() + before, data_len));
            data_len += track.pending.iter().map(|(d, _, _)| d.len() as u64).sum::<u64>();
            trafs.push(traf);
        }
        let mut mfhd = Vec::new();
        put32(&mut mfhd, self.sequence);
        let mfhd = full(b"mfhd", 0, 0, &mfhd);
        if self.fragmented {
            let mut moof = boxes(b"moof", &[&[mfhd][..], &trafs[..]].concat());
            let head = moof.len() - trafs.iter().map(|t| t.len()).sum::<usize>();
            let moof_len = moof.len() as u64;
            for (at, offset) in patches {
                let spot = head + at;
                let value = (moof_len + 8 + offset) as u32;
                moof[spot..spot + 4].copy_from_slice(&value.to_be_bytes());
            }
            self.fragments.push(self.at);
            self.write(&moof)?;
        }
        let mdat_len = data_len + 8;
        let mut header = Vec::new();
        put32(&mut header, mdat_len as u32);
        header.extend_from_slice(b"mdat");
        self.write(&header)?;
        for i in 0..self.tracks.len() {
            let pending = std::mem::take(&mut self.tracks[i].pending);
            if pending.is_empty() {
                continue;
            }
            let start = self.at;
            let count = pending.len() as u32;
            for (data, span, key) in &pending {
                self.write(data)?;
                let track = &mut self.tracks[i];
                track.sizes.push(data.len() as u32);
                track.durations.push(*span);
                if *key {
                    track.keys.push(track.sizes.len() as u32);
                }
                track.decoded += *span as u64;
            }
            self.tracks[i].chunks.push((start, count));
        }
        if self.fragmented {
            self.file.flush()?;
        }
        Ok(())
    }

    pub fn finish(mut self) -> std::io::Result<f64> {
        if !self.started {
            drop(self.file);
            return Ok(0.0);
        }
        for track in &mut self.tracks {
            track.release_held();
        }
        self.flush()?;
        let moov = self.moov(true);
        self.write(&moov)?;
        self.file.flush()?;
        let file = self.file.get_mut();
        for at in self.fragments.iter().copied().chain(self.fragmented.then_some(self.init_at)) {
            file.seek(SeekFrom::Start(at + 4))?;
            file.write_all(b"free")?;
        }
        file.flush()?;
        Ok(self.tracks[0].duration() as f64 / VIDEO_SCALE as f64)
    }

}

pub fn write_clip(path: &Path, codec: Codec, size: (u32, u32), fps: u32, sound: Option<Sound>, video: &[Packet], audio: &[Packet]) -> std::io::Result<f64> {
    let Some(first) = video.iter().position(|p| p.key) else { return Ok(0.0) };
    let zero = video[first].pts;
    let mut mp4 = Mp4::create(path, codec, size, fps, sound, false)?;
    let mut audio = audio.iter().filter(|p| p.pts >= zero).peekable();
    for packet in &video[first..] {
        while let Some(sound) = audio.next_if(|a| a.pts <= packet.pts) {
            mp4.audio(&Packet { pts: sound.pts - zero, ..sound.clone() })?;
        }
        mp4.video(&Packet { pts: packet.pts - zero, ..packet.clone() })?;
    }
    for sound in audio {
        mp4.audio(&Packet { pts: sound.pts - zero, ..sound.clone() })?;
    }
    mp4.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h264_key() -> Vec<u8> {
        let mut data = vec![0, 0, 0, 1, 0x67, 100, 0, 40, 0xAC, 0xD9];
        data.extend_from_slice(&[0, 0, 0, 1, 0x68, 0xEB, 0xE3, 0xCB]);
        data.extend_from_slice(&[0, 0, 1, 0x65, 0x88, 0x84, 0x00, 0x33]);
        data
    }

    #[test]
    fn annex_b_splits_and_finds_keys() {
        let key = h264_key();
        let units = nals(&key);
        assert_eq!(units.len(), 3);
        assert_eq!(units[0][0] & 0x1F, 7);
        assert!(is_key(Codec::H264, &key));
        assert!(!is_key(Codec::H264, &[0, 0, 0, 1, 0x41, 0x9A]));
        let video = Video::from_key(Codec::H264, (1920, 1080), &key).unwrap();
        let sample = video.sample(&key);
        assert_eq!(&sample[..4], &[0, 0, 0, 5]);
        assert_eq!(sample[4], 0x65);
        assert_eq!(unescape(&[0, 0, 3, 1, 0, 0, 3]), vec![0, 0, 1, 0, 0]);
    }

    fn walk(data: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        let mut i = 0;
        while i + 8 <= data.len() {
            let size = u32::from_be_bytes(data[i..i + 4].try_into().unwrap()) as usize;
            out.push(String::from_utf8_lossy(&data[i + 4..i + 8]).into_owned());
            if size < 8 {
                break;
            }
            i += size;
        }
        out
    }

    #[test]
    fn writes_a_regular_mp4_after_the_fragments() {
        let path = std::env::temp_dir().join("cocuyo-mp4-prueba.mp4");
        let mut mp4 = Mp4::create(&path, Codec::H264, (1920, 1080), 60, Some(Sound::Aac(192_000)), true).unwrap();
        let key: Arc<[u8]> = h264_key().into();
        let delta: Arc<[u8]> = vec![0, 0, 0, 1, 0x41, 0x9A, 0x22].into();
        for i in 0..150i64 {
            let pts = i * TICKS / 60;
            mp4.audio(&Packet { data: vec![0x21, 0x10, 0x04].into(), pts: i * 1024 * TICKS / 48_000, key: true }).unwrap();
            mp4.video(&Packet { data: if i % 60 == 0 { key.clone() } else { delta.clone() }, pts, key: i % 60 == 0 }).unwrap();
        }
        let seconds = mp4.finish().unwrap();
        assert!((seconds - 2.5).abs() < 0.02, "{seconds}");
        let bytes = std::fs::read(&path).unwrap();
        let top = walk(&bytes);
        assert_eq!(top.first().map(String::as_str), Some("ftyp"));
        assert_eq!(top.last().map(String::as_str), Some("moov"));
        assert!(top.iter().filter(|b| *b == "mdat").count() >= 2);
        assert!(!top.iter().any(|b| b == "moof"));
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("avcC") && text.contains("esds") && text.contains("stss") && text.contains("co64"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_clip_starts_at_a_key_frame() {
        let path = std::env::temp_dir().join("cocuyo-clip-prueba.mp4");
        let key: Arc<[u8]> = h264_key().into();
        let delta: Arc<[u8]> = vec![0, 0, 0, 1, 0x41, 0x9A, 0x22].into();
        let video: Vec<Packet> = (0..90i64).map(|i| Packet { data: if i % 30 == 0 { key.clone() } else { delta.clone() }, pts: i * TICKS / 30, key: i % 30 == 0 }).collect();
        let seconds = write_clip(&path, Codec::H264, (1280, 720), 30, None, &video[10..], &[]).unwrap();
        assert!((seconds - 2.0).abs() < 0.05, "{seconds}");
        let _ = std::fs::remove_file(&path);
    }
}
