// le quita el ruido al microfono: el ventilador, el zumbido, el soplido de fondo. fft propia, sin crates, menos de un 1 % de un nucleo

use std::f32::consts::PI;

const SIZE: usize = 512;
const HOP: usize = SIZE / 2;

pub struct Denoiser {
    floor: f32,
    over: f32,
    window: Vec<f32>,
    pending: Vec<f32>,
    tail: Vec<f32>,
    noise: Vec<f32>,
    smooth: Vec<f32>,
    previous: Vec<f32>,
    frames: u32,
    re: Vec<f32>,
    im: Vec<f32>,
    cos: Vec<f32>,
    sin: Vec<f32>,
    order: Vec<usize>,
}

impl Denoiser {
    pub fn new(strong: bool) -> Denoiser {
        let bits = SIZE.trailing_zeros();
        let bins = SIZE / 2 + 1;
        Denoiser {
            floor: if strong { 0.06 } else { 0.18 },
            over: if strong { 1.6 } else { 1.0 },
            window: (0..SIZE).map(|i| (PI * i as f32 / SIZE as f32).sin()).collect(),
            pending: Vec::with_capacity(SIZE * 4),
            tail: vec![0.0; SIZE],
            noise: vec![0.0; bins],
            smooth: vec![0.0; bins],
            previous: vec![1.0; bins],
            frames: 0,
            re: vec![0.0; SIZE],
            im: vec![0.0; SIZE],
            cos: (0..SIZE / 2).map(|i| (2.0 * PI * i as f32 / SIZE as f32).cos()).collect(),
            sin: (0..SIZE / 2).map(|i| (2.0 * PI * i as f32 / SIZE as f32).sin()).collect(),
            order: (0..SIZE).map(|i| i.reverse_bits() >> (usize::BITS - bits)).collect(),
        }
    }

    fn fft(&mut self, inverse: bool) {
        for i in 0..SIZE {
            let j = self.order[i];
            if i < j {
                self.re.swap(i, j);
                self.im.swap(i, j);
            }
        }
        let sign = if inverse { 1.0 } else { -1.0 };
        let mut length = 2;
        while length <= SIZE {
            let half = length / 2;
            let stride = SIZE / length;
            for start in (0..SIZE).step_by(length) {
                for k in 0..half {
                    let (c, s) = (self.cos[k * stride], sign * self.sin[k * stride]);
                    let (a, b) = (start + k, start + k + half);
                    let tr = self.re[b] * c - self.im[b] * s;
                    let ti = self.re[b] * s + self.im[b] * c;
                    self.re[b] = self.re[a] - tr;
                    self.im[b] = self.im[a] - ti;
                    self.re[a] += tr;
                    self.im[a] += ti;
                }
            }
            length *= 2;
        }
    }

    fn frame(&mut self, out: &mut Vec<f32>) {
        for i in 0..SIZE {
            self.re[i] = self.pending[i] * self.window[i];
            self.im[i] = 0.0;
        }
        self.fft(false);
        self.frames = self.frames.saturating_add(1);
        let learning = self.frames <= 12;
        for k in 0..=SIZE / 2 {
            let power = self.re[k] * self.re[k] + self.im[k] * self.im[k];
            self.smooth[k] = if self.frames == 1 { power } else { 0.75 * self.smooth[k] + 0.25 * power };
            if learning {
                self.noise[k] = if self.frames == 1 { power } else { self.noise[k] + (power - self.noise[k]) / self.frames as f32 };
            } else if self.smooth[k] < self.noise[k] {
                self.noise[k] = self.smooth[k];
            } else {
                self.noise[k] *= 1.0025;
            }
            let noise = self.noise[k].max(1e-12);
            let post = power / noise;
            let prior = 0.97 * self.previous[k] + 0.03 * (post - 1.0).max(0.0);
            let gain = (prior / (prior + self.over)).max(self.floor);
            self.previous[k] = gain * gain * post;
            self.re[k] *= gain;
            self.im[k] *= gain;
            if k > 0 && k < SIZE / 2 {
                self.re[SIZE - k] = self.re[k];
                self.im[SIZE - k] = -self.im[k];
            }
        }
        self.fft(true);
        for i in 0..SIZE {
            self.tail[i] += self.re[i] / SIZE as f32 * self.window[i];
        }
        out.extend_from_slice(&self.tail[..HOP]);
        self.tail.copy_within(HOP.., 0);
        self.tail[SIZE - HOP..].fill(0.0);
        self.pending.drain(..HOP);
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        self.pending.extend_from_slice(input);
        let mut out = Vec::with_capacity(input.len() + HOP);
        while self.pending.len() >= SIZE {
            self.frame(&mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(seed: &mut u32) -> f32 {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 17;
        *seed ^= *seed << 5;
        (*seed as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn rms(samples: &[f32]) -> f32 {
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len().max(1) as f32).sqrt()
    }

    #[test]
    fn it_lowers_the_hiss_and_keeps_the_voice() {
        let rate = 48_000.0;
        let mut seed = 7;
        let hiss: Vec<f32> = (0..48_000 * 3).map(|_| noise(&mut seed) * 0.05).collect();
        let voice: Vec<f32> = (0..48_000 * 3).map(|i| if i > 48_000 * 2 { (2.0 * PI * 220.0 * i as f32 / rate).sin() * 0.4 } else { 0.0 }).collect();
        let mixed: Vec<f32> = hiss.iter().zip(&voice).map(|(a, b)| a + b).collect();
        for strong in [false, true] {
            let mut denoiser = Denoiser::new(strong);
            let started = std::time::Instant::now();
            let cleaned = denoiser.process(&mixed);
            let spent = started.elapsed().as_secs_f64();
            assert!(spent < 0.3, "3 s de audio en {spent} s");
            println!("[ruido] {} 3 s de audio en {:.2} ms ({:.3} % de un núcleo)", if strong { "fuerte" } else { "suave" }, spent * 1000.0, spent / 3.0 * 100.0);
            let quiet_before = rms(&mixed[48_000..96_000]);
            let quiet_after = rms(&cleaned[48_000..96_000]);
            assert!(quiet_after < quiet_before * if strong { 0.2 } else { 0.4 }, "ruido {quiet_before} -> {quiet_after}");
            let voiced_after = rms(&cleaned[110_000..140_000]);
            assert!(voiced_after > rms(&voice[110_000..140_000]) * 0.85, "voz {voiced_after}");
        }
    }

    #[test]
    fn silence_stays_silent_and_length_is_kept() {
        let mut denoiser = Denoiser::new(true);
        let out: Vec<f32> = (0..20).flat_map(|_| denoiser.process(&[0.0; 480])).collect();
        assert!(out.iter().all(|s| s.abs() < 1e-6));
        assert_eq!(out.len(), (20 * 480 - SIZE) / HOP * HOP + HOP);
    }
}
