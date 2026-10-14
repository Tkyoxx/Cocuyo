// el dibujo de la pildora, pixel a pixel, para que se vea igual que fuera del juego

#[derive(Clone, Copy, Default)]
pub struct Paint {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Paint {
    pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> Self {
        Self { r: r as f32 / 255.0 * a, g: g as f32 / 255.0 * a, b: b as f32 / 255.0 * a, a }
    }

    pub fn under(self, top: Paint) -> Paint {
        let keep = 1.0 - top.a;
        Paint { r: top.r + self.r * keep, g: top.g + self.g * keep, b: top.b + self.b * keep, a: top.a + self.a * keep }
    }

    pub fn fade(self, alpha: f32) -> Paint {
        Paint { r: self.r * alpha, g: self.g * alpha, b: self.b * alpha, a: self.a * alpha }
    }
}

pub const UI: [f32; 4] = [0.25, 1.0, 0.5, 1.0];
pub const POP: [f32; 4] = [0.3, 1.8, 0.55, 1.0];
pub const OUT: [f32; 4] = [0.16, 1.0, 0.3, 1.0];

pub fn ease(curve: [f32; 4], time: f32) -> f32 {
    let time = time.clamp(0.0, 1.0);
    let [x1, y1, x2, y2] = curve;
    let bezier = |a: f32, b: f32, t: f32| 3.0 * (1.0 - t) * (1.0 - t) * t * a + 3.0 * (1.0 - t) * t * t * b + t * t * t;
    let (mut low, mut high) = (0.0, 1.0);
    for _ in 0..18 {
        let mid = (low + high) / 2.0;
        if bezier(x1, x2, mid) < time {
            low = mid;
        } else {
            high = mid;
        }
    }
    bezier(y1, y2, (low + high) / 2.0)
}

fn rounded(x: f32, y: f32, half_w: f32, half_h: f32, radius: f32) -> f32 {
    let radius = radius.min(half_w).min(half_h);
    let (qx, qy) = (x.abs() - half_w + radius, y.abs() - half_h + radius);
    qx.max(qy).min(0.0) + (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() - radius
}

fn cover(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

fn shadow(x: f32, y: f32, half_w: f32, half_h: f32, radius: f32, dy: f32, blur: f32, spread: f32, color: Paint) -> Paint {
    let distance = rounded(x, y - dy, half_w + spread, half_h + spread, (radius + spread).max(0.0));
    let sigma = (blur / 2.0).max(0.01);
    color.fade(0.5 - 0.5 * (distance / (sigma * 1.6)).tanh())
}

pub const PILL_W: f32 = 76.0;
pub const PILL_H: f32 = 34.0;
pub const PILL_MARGIN: f32 = 34.0;
const BLUE: (u8, u8, u8) = (0x5c, 0xb8, 0xff);
const BAR_GAIN: [f32; 5] = [0.5, 0.85, 1.0, 0.7, 0.45];

pub struct PillLook {
    pub opacity: f32,
    pub drop: f32,
    pub scale: f32,
    pub bars: [(f32, f32); 5],
    pub wave: f32,
    pub wave_scale: f32,
    pub check: f32,
    pub check_scale: f32,
    pub check_drawn: f32,
}

pub fn listening_bars(level: f32) -> [(f32, f32); 5] {
    BAR_GAIN.map(|gain| (0.22 + level * gain * 0.78, 1.0))
}

pub fn thinking_bars(seconds: f32) -> [(f32, f32); 5] {
    std::array::from_fn(|i| {
        let phase = ((seconds - 0.1 * i as f32) / 0.95).rem_euclid(1.0);
        let lift = if phase < 0.3 {
            phase / 0.3
        } else if phase < 0.6 {
            1.0 - (phase - 0.3) / 0.3
        } else {
            0.0
        };
        let lift = lift * lift * (3.0 - 2.0 * lift);
        (0.22 + 0.68 * lift, 0.45 + 0.55 * lift)
    })
}

pub fn pill(look: &PillLook, x: f32, y: f32) -> Paint {
    if look.opacity <= 0.0 {
        return Paint::default();
    }
    let (x, y) = (x / look.scale, (y - look.drop) / look.scale);
    let (half_w, half_h) = (PILL_W / 2.0, PILL_H / 2.0);
    let mut out = shadow(x, y, half_w, half_h, half_h, 8.0, 22.0, 0.0, Paint::rgba(0, 0, 0, 0.4));

    let edge = rounded(x, y, half_w, half_h, half_h);
    let body = cover(edge);
    if body > 0.0 {
        out = out.under(Paint::rgba(20, 20, 22, 1.0).fade(body));
        let rim = body * (1.0 - cover(edge + 1.0));
        out = out.under(Paint::rgba(255, 255, 255, 0.1).fade(rim));

        if look.wave > 0.0 {
            let (wx, wy) = (x / look.wave_scale, y / look.wave_scale);
            for (i, (height, opacity)) in look.bars.iter().enumerate() {
                let center = (i as f32 - 2.0) * 6.0;
                let bar = cover(rounded(wx - center, wy, 1.5, 8.0 * height, 1.5));
                if bar > 0.0 {
                    out = out.under(Paint::rgba(BLUE.0, BLUE.1, BLUE.2, 1.0).fade(bar * opacity * look.wave));
                }
            }
        }
        if look.check > 0.0 {
            let unit = 20.0 / 24.0 * look.check_scale;
            let (cx, cy) = (x / unit + 12.0, y / unit + 12.0);
            let points = [(5.0, 12.5), (9.5, 17.0), (19.0, 7.5)];
            let lengths = [6.364, 13.435];
            let mut drawn = look.check_drawn * 24.0;
            let mut stroke: f32 = 0.0;
            for (segment, length) in lengths.iter().enumerate() {
                let reach = (drawn / length).clamp(0.0, 1.0);
                drawn -= length;
                if reach <= 0.0 {
                    break;
                }
                let (ax, ay) = points[segment];
                let (bx, by) = (ax + (points[segment + 1].0 - ax) * reach, ay + (points[segment + 1].1 - ay) * reach);
                let (dx, dy) = (bx - ax, by - ay);
                let along = (((cx - ax) * dx + (cy - ay) * dy) / (dx * dx + dy * dy).max(0.0001)).clamp(0.0, 1.0);
                let distance = ((cx - ax - dx * along).powi(2) + (cy - ay - dy * along).powi(2)).sqrt();
                stroke = stroke.max(((1.3 - distance) * unit + 0.5).clamp(0.0, 1.0));
            }
            out = out.under(Paint::rgba(BLUE.0, BLUE.1, BLUE.2, 1.0).fade(stroke * look.check));
        }
    }
    out.fade(look.opacity)
}

pub const SAID_RADIUS: f32 = 20.0;
pub const SAID_PAD_X: f32 = 18.0;
pub const SAID_PAD_Y: f32 = 9.0;
pub const SAID_FONT: f32 = 16.0;
pub const SAID_LINE: f32 = 22.4;
pub const SAID_MARGIN: f32 = 46.0;

pub fn bubble(dark: bool, x: f32, y: f32, w: f32, h: f32) -> Paint {
    let (half_w, half_h) = (w / 2.0, h / 2.0);
    let edge = rounded(x, y, half_w, half_h, SAID_RADIUS);
    let body = cover(edge);
    let down = ((y + half_h) / h).clamp(0.0, 1.0);
    if dark {
        let mut out = shadow(x, y, half_w, half_h, SAID_RADIUS, 12.0, 34.0, 0.0, Paint::rgba(0, 0, 0, 0.5));
        if body > 0.0 {
            out = out.under(Paint::rgba(16, 16, 20, 0.94).fade(body));
            let rim = body * (1.0 - cover(edge + 1.0));
            out = out.under(Paint::rgba(255, 255, 255, 0.14).fade(rim));
            let shine = cover(edge + 1.0) * (1.0 - cover(rounded(x, y - 1.0, half_w - 1.0, half_h - 1.0, SAID_RADIUS - 1.0)));
            out = out.under(Paint::rgba(255, 255, 255, 0.1).fade(shine));
        }
        out
    } else {
        let mut out = shadow(x, y, half_w, half_h, SAID_RADIUS, 14.0, 30.0, -6.0, Paint::rgba(15, 23, 42, 0.26));
        out = out.under(shadow(x, y, half_w, half_h, SAID_RADIUS, 1.0, 2.0, 0.0, Paint::rgba(15, 23, 42, 0.16)));
        if body > 0.0 {
            out = out.under(Paint::rgba(255, 255, 255, 0.96 - 0.06 * down).fade(body));
            let rim = body * (1.0 - cover(edge + 1.2));
            let rim_color = if down < 0.42 {
                Paint::rgba(255, 255, 255, 1.0 - 0.5 * down / 0.42)
            } else {
                let t = (down - 0.42) / 0.58;
                Paint::rgba((255.0 - 215.0 * t) as u8, (255.0 - 215.0 * t) as u8, (255.0 - 205.0 * t) as u8, 0.5 - 0.38 * t)
            };
            out = out.under(rim_color.fade(rim));
            let shine = cover(edge + 1.2) * (1.0 - cover(rounded(x, y - 1.2, half_w - 1.2, half_h - 1.2, SAID_RADIUS - 1.2)));
            out = out.under(Paint::rgba(255, 255, 255, 0.95).fade(shine));
        }
        out
    }
}

pub fn ink(dark: bool) -> (u8, u8, u8) {
    if dark {
        (244, 244, 245)
    } else {
        (0x1c, 0x1b, 0x17)
    }
}
