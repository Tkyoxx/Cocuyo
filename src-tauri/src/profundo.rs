// el ruido fuera, como krisp: deepfilternet3 escrito a mano, cuadro a cuadro de 10 ms, sin crates ni onnx

use std::collections::HashMap;
use std::f32::consts::PI;
use std::sync::{Arc, Mutex, OnceLock, Weak};

pub const RATE: usize = 48_000;
pub const HOP: usize = 480;
const FFT: usize = 960;
const FREQS: usize = FFT / 2 + 1;
const ERB: usize = 32;
const DF: usize = 96;
const ORDER: usize = 5;
const LOOK: usize = 2;
const CH: usize = 64;
const HID: usize = 256;
const ALPHA: f32 = 0.99;
pub const DELAY: usize = FFT - HOP + LOOK * HOP;

static BLOB: &[u8] = include_bytes!("../modelos/dfn3.bin");

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Level {
    Soft,
    Normal,
    Max,
}

fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = ((bits >> 10) & 0x1f) as i32;
    let frac = (bits & 0x3ff) as f32;
    match exp {
        0 => sign * frac * (1.0 / 16_777_216.0),
        31 => sign * f32::INFINITY,
        _ => sign * (1.0 + frac / 1024.0) * 2f32.powi(exp - 15),
    }
}

fn read_blob() -> HashMap<String, (Vec<usize>, Vec<u16>)> {
    let mut at = 8;
    let take = |at: &mut usize, n: usize| {
        let part = &BLOB[*at..*at + n];
        *at += n;
        part
    };
    let count = u32::from_le_bytes(BLOB[4..8].try_into().unwrap()) as usize;
    let mut out = HashMap::with_capacity(count);
    for _ in 0..count {
        let len = u16::from_le_bytes(take(&mut at, 2).try_into().unwrap()) as usize;
        let name = String::from_utf8_lossy(take(&mut at, len)).into_owned();
        let dims = take(&mut at, 1)[0] as usize;
        let shape: Vec<usize> = (0..dims).map(|_| u32::from_le_bytes(take(&mut at, 4).try_into().unwrap()) as usize).collect();
        let size: usize = shape.iter().product();
        let values = take(&mut at, size * 2).chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        out.insert(name, (shape, values));
    }
    out
}

enum Mat {
    Byte(Vec<i8>, Vec<f32>),
    Half(Vec<u16>),
    Full(Vec<f32>),
}

fn quantize(values: &[f32], cols: usize) -> Mat {
    let mut q = Vec::with_capacity(values.len());
    let mut scales = Vec::with_capacity(values.len() / cols);
    for row in values.chunks_exact(cols) {
        let top = row.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let scale = if top > 0.0 { top / 127.0 } else { 1.0 };
        q.extend(row.iter().map(|v| (v / scale).round().clamp(-127.0, 127.0) as i8));
        scales.push(scale);
    }
    Mat::Byte(q, scales)
}

struct Gru {
    w: Mat,
    r: Mat,
    b: Vec<f32>,
}

struct Net {
    erb0: Vec<f32>,
    erb0_b: Vec<f32>,
    erb_dw: [Vec<f32>; 3],
    erb_pw: [Vec<f32>; 3],
    erb_pb: [Vec<f32>; 3],
    df0: Vec<f32>,
    df0_pw: Vec<f32>,
    df0_pb: Vec<f32>,
    df1_dw: Vec<f32>,
    df1_pw: Vec<f32>,
    df1_pb: Vec<f32>,
    fc_emb: Vec<f32>,
    enc_in: Vec<f32>,
    enc_gru: Gru,
    enc_out: Vec<f32>,
    lsnr_w: Vec<f32>,
    lsnr_b: f32,
    erb_in: Vec<f32>,
    erb_gru: [Gru; 2],
    erb_out: Vec<f32>,
    skip_w: [Vec<f32>; 4],
    skip_b: [Vec<f32>; 4],
    up_dw: [Vec<f32>; 3],
    up_pw: [Vec<f32>; 3],
    up_pb: [Vec<f32>; 3],
    mask_w: Vec<f32>,
    mask_b: f32,
    df_in: Vec<f32>,
    df_gru: [Gru; 2],
    df_skip: Vec<f32>,
    convp: Vec<f32>,
    convp_pw: Vec<f32>,
    convp_pb: Vec<f32>,
    df_out: Vec<f32>,
}

impl Net {
    fn load(precise: bool) -> Net {
        let all = std::cell::RefCell::new(read_blob());
        let raw = |name: &str, size: usize| {
            let (shape, values) = all.borrow_mut().remove(name).unwrap_or_else(|| panic!("falta {name} en dfn3.bin"));
            assert_eq!(values.len(), size, "{name} {shape:?}");
            values
        };
        let get = |name: &str, size: usize| raw(name, size).into_iter().map(half).collect::<Vec<f32>>();
        let mat = |name: &str| {
            let bits = raw(name, 3 * HID * HID);
            match (fast(), precise) {
                (true, true) => Mat::Half(bits),
                (_, false) => quantize(&bits.into_iter().map(half).collect::<Vec<f32>>(), HID),
                (false, true) => Mat::Full(bits.into_iter().map(half).collect()),
            }
        };
        let gru = |name: &str| Gru { w: mat(&format!("{name}.W")), r: mat(&format!("{name}.R")), b: get(&format!("{name}.B"), 6 * HID) };
        let enc_gru = gru("enc.emb_gru.GRU");
        let erb_gru = [gru("erb.emb_gru.GRU"), gru("erb.emb_gru.GRU_1")];
        let df_gru = [gru("df.df_gru.gru.GRU"), gru("df.df_gru.gru.GRU_1")];
        let raw_convp = get("df.df_convp.df_convp.1.Conv.w", 10 * 32 * 5);
        let mut convp = vec![0.0; 10 * 160];
        for o in 0..10 {
            for ci in 0..32 {
                for kt in 0..5 {
                    convp[o * 160 + kt * 32 + ci] = raw_convp[o * 160 + ci * 5 + kt];
                }
            }
        }
        Net {
            erb0: get("enc.erb_conv0.1.Conv.w", CH * 9),
            erb0_b: get("enc.erb_conv0.1.Conv.b", CH),
            erb_dw: [get("enc.erb_conv1.0.Conv.w", CH * 3), get("enc.erb_conv2.Conv.w", CH * 3), get("enc.erb_conv3.0.Conv.w", CH * 3)],
            erb_pw: [get("enc.erb_conv1.1.Conv.w", CH * CH), get("enc.erb_conv2.Conv_1.w", CH * CH), get("enc.erb_conv3.1.Conv.w", CH * CH)],
            erb_pb: [get("enc.erb_conv1.1.Conv.b", CH), get("enc.erb_conv2.Conv_1.b", CH), get("enc.erb_conv3.1.Conv.b", CH)],
            df0: get("enc.df_conv0.1.Conv.w", CH * 9),
            df0_pw: get("enc.df_conv0.2.Conv.w", CH * CH),
            df0_pb: get("enc.df_conv0.2.Conv.b", CH),
            df1_dw: get("enc.df_conv1.Conv.w", CH * 3),
            df1_pw: get("enc.df_conv1.Conv_1.w", CH * CH),
            df1_pb: get("enc.df_conv1.Conv_1.b", CH),
            fc_emb: get("enc.df_fc_emb.0.Einsum.w", 32 * 96 * 16),
            enc_in: get("enc.emb_gru.linear_in.0.Einsum.w", 16 * 32 * 16),
            enc_gru,
            enc_out: get("enc.emb_gru.linear_out.0.Einsum.w", 16 * 16 * 32),
            lsnr_w: get("enc.lsnr_fc.0.MatMul.w", 512),
            lsnr_b: get("enc.lsnr_fc.0.Add.b", 1)[0],
            erb_in: get("erb.emb_gru.linear_in.Einsum.w", 16 * 32 * 16),
            erb_gru,
            erb_out: get("erb.emb_gru.linear_out.Einsum.w", 16 * 16 * 32),
            skip_w: [get("erb.conv3p.0.Conv.w", CH), get("erb.conv2p.Conv.w", CH), get("erb.conv1p.Conv.w", CH), get("erb.conv0p.Conv.w", CH)],
            skip_b: [get("erb.conv3p.0.Conv.b", CH), get("erb.conv2p.Conv.b", CH), get("erb.conv1p.Conv.b", CH), get("erb.conv0p.Conv.b", CH)],
            up_dw: [get("erb.convt3.Conv.w", CH * 3), get("erb.convt2.0.ConvTranspose.w", CH * 3), get("erb.convt1.ConvTranspose.w", CH * 3)],
            up_pw: [get("erb.convt3.Conv_1.w", CH * CH), get("erb.convt2.1.Conv.w", CH * CH), get("erb.convt1.Conv.w", CH * CH)],
            up_pb: [get("erb.convt3.Conv_1.b", CH), get("erb.convt2.1.Conv.b", CH), get("erb.convt1.Conv.b", CH)],
            mask_w: get("erb.conv0_out.0.Conv.w", CH * 3),
            mask_b: get("erb.conv0_out.0.Conv.b", 1)[0],
            df_in: get("df.df_gru.linear_in.linear_in.0.Einsum.w", 8 * 64 * 32),
            df_gru,
            df_skip: get("df.df_skip.Einsum.w", 16 * 32 * 16),
            convp,
            convp_pw: get("df.df_convp.df_convp.2.Conv.w", 100),
            convp_pb: get("df.df_convp.df_convp.2.Conv.b", 10),
            df_out: get("df.df_out.df_out.0.Einsum.w", 16 * 16 * 60),
        }
    }
}

fn shared(precise: bool) -> Arc<Net> {
    static CACHE: Mutex<[Weak<Net>; 2]> = Mutex::new([Weak::new(), Weak::new()]);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(net) = cache[precise as usize].upgrade() {
        return net;
    }
    let net = Arc::new(Net::load(precise));
    cache[precise as usize] = Arc::downgrade(&net);
    net
}

pub fn fast() -> bool {
    static FAST: OnceLock<bool> = OnceLock::new();
    *FAST.get_or_init(|| {
        #[cfg(target_arch = "x86_64")]
        return std::env::var_os("COCUYO_SIN_AVX").is_none() && std::arch::is_x86_feature_detected!("avx2") && std::arch::is_x86_feature_detected!("fma");
        #[allow(unreachable_code)]
        false
    })
}

mod kernel {
    #[cfg(target_arch = "x86_64")]
    use std::arch::x86_64::*;

    pub fn matvec(w: &[f32], x: &[f32], bias: &[f32], out: &mut [f32]) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            if super::fast() {
                matvec_avx(w, x, bias, out)
            } else {
                matvec_sse(w, x, bias, out)
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let cols = x.len();
            for (r, o) in out.iter_mut().enumerate() {
                *o = bias[r] + w[r * cols..(r + 1) * cols].iter().zip(x).map(|(a, b)| a * b).sum::<f32>();
            }
        }
    }

    pub fn matvec_byte(w: &[i8], scales: &[f32], x: &[f32], bias: &[f32], out: &mut [f32]) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            if super::fast() {
                matvec_byte_avx(w, scales, x, bias, out)
            } else {
                matvec_byte_sse(w, scales, x, bias, out)
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let cols = x.len();
            for (r, o) in out.iter_mut().enumerate() {
                *o = bias[r] + scales[r] * w[r * cols..(r + 1) * cols].iter().zip(x).map(|(a, b)| *a as f32 * b).sum::<f32>();
            }
        }
    }

    pub fn axpy(y: &mut [f32], a: f32, x: &[f32]) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            if super::fast() {
                axpy_avx(y, a, x)
            } else {
                axpy_sse(y, a, x)
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        for (y, x) in y.iter_mut().zip(x) {
            *y += a * x;
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn axpy_avx(y: &mut [f32], a: f32, x: &[f32]) {
        let n = y.len().min(x.len());
        let (yp, xp) = (y.as_mut_ptr(), x.as_ptr());
        let k = _mm256_set1_ps(a);
        let mut i = 0;
        while i + 8 <= n {
            _mm256_storeu_ps(yp.add(i), _mm256_fmadd_ps(k, _mm256_loadu_ps(xp.add(i)), _mm256_loadu_ps(yp.add(i))));
            i += 8;
        }
        while i < n {
            *yp.add(i) += a * *xp.add(i);
            i += 1;
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn matvec_byte_avx(w: &[i8], scales: &[f32], x: &[f32], bias: &[f32], out: &mut [f32]) {
        let cols = x.len();
        let rows = out.len();
        assert!(cols % 16 == 0 && w.len() >= rows * cols && bias.len() >= rows && scales.len() >= rows && rows % 4 == 0);
        let xp = x.as_ptr();
        let mut r = 0;
        while r < rows {
            let p0 = w.as_ptr().add(r * cols);
            let (p1, p2, p3) = (p0.add(cols), p0.add(2 * cols), p0.add(3 * cols));
            let z = _mm256_setzero_ps();
            let (mut a0, mut b0, mut a1, mut b1, mut a2, mut b2, mut a3, mut b3) = (z, z, z, z, z, z, z, z);
            let mut c = 0;
            while c < cols {
                let lo = _mm256_loadu_ps(xp.add(c));
                let hi = _mm256_loadu_ps(xp.add(c + 8));
                let q0 = _mm_loadu_si128(p0.add(c) as *const __m128i);
                let q1 = _mm_loadu_si128(p1.add(c) as *const __m128i);
                let q2 = _mm_loadu_si128(p2.add(c) as *const __m128i);
                let q3 = _mm_loadu_si128(p3.add(c) as *const __m128i);
                a0 = _mm256_fmadd_ps(_mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(q0)), lo, a0);
                b0 = _mm256_fmadd_ps(_mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(_mm_srli_si128(q0, 8))), hi, b0);
                a1 = _mm256_fmadd_ps(_mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(q1)), lo, a1);
                b1 = _mm256_fmadd_ps(_mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(_mm_srli_si128(q1, 8))), hi, b1);
                a2 = _mm256_fmadd_ps(_mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(q2)), lo, a2);
                b2 = _mm256_fmadd_ps(_mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(_mm_srli_si128(q2, 8))), hi, b2);
                a3 = _mm256_fmadd_ps(_mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(q3)), lo, a3);
                b3 = _mm256_fmadd_ps(_mm256_cvtepi32_ps(_mm256_cvtepi8_epi32(_mm_srli_si128(q3, 8))), hi, b3);
                c += 16;
            }
            out[r] = bias[r] + scales[r] * sum8(_mm256_add_ps(a0, b0));
            out[r + 1] = bias[r + 1] + scales[r + 1] * sum8(_mm256_add_ps(a1, b1));
            out[r + 2] = bias[r + 2] + scales[r + 2] * sum8(_mm256_add_ps(a2, b2));
            out[r + 3] = bias[r + 3] + scales[r + 3] * sum8(_mm256_add_ps(a3, b3));
            r += 4;
        }
    }

    pub fn matvec_half(w: &[u16], x: &[f32], bias: &[f32], out: &mut [f32]) {
        #[cfg(target_arch = "x86_64")]
        if super::fast() {
            return unsafe { matvec_half_avx(w, x, bias, out) };
        }
        let cols = x.len();
        for (r, o) in out.iter_mut().enumerate() {
            *o = bias[r] + w[r * cols..(r + 1) * cols].iter().zip(x).map(|(a, b)| super::half(*a) * b).sum::<f32>();
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma,f16c")]
    unsafe fn matvec_half_avx(w: &[u16], x: &[f32], bias: &[f32], out: &mut [f32]) {
        let cols = x.len();
        let rows = out.len();
        assert!(cols % 16 == 0 && w.len() >= rows * cols && bias.len() >= rows && rows % 4 == 0);
        let xp = x.as_ptr();
        let mut r = 0;
        while r < rows {
            let p0 = w.as_ptr().add(r * cols);
            let (p1, p2, p3) = (p0.add(cols), p0.add(2 * cols), p0.add(3 * cols));
            let z = _mm256_setzero_ps();
            let (mut a0, mut b0, mut a1, mut b1, mut a2, mut b2, mut a3, mut b3) = (z, z, z, z, z, z, z, z);
            let mut c = 0;
            while c < cols {
                let lo = _mm256_loadu_ps(xp.add(c));
                let hi = _mm256_loadu_ps(xp.add(c + 8));
                let h = |p: *const u16| _mm_loadu_si128(p as *const __m128i);
                a0 = _mm256_fmadd_ps(_mm256_cvtph_ps(h(p0.add(c))), lo, a0);
                b0 = _mm256_fmadd_ps(_mm256_cvtph_ps(h(p0.add(c + 8))), hi, b0);
                a1 = _mm256_fmadd_ps(_mm256_cvtph_ps(h(p1.add(c))), lo, a1);
                b1 = _mm256_fmadd_ps(_mm256_cvtph_ps(h(p1.add(c + 8))), hi, b1);
                a2 = _mm256_fmadd_ps(_mm256_cvtph_ps(h(p2.add(c))), lo, a2);
                b2 = _mm256_fmadd_ps(_mm256_cvtph_ps(h(p2.add(c + 8))), hi, b2);
                a3 = _mm256_fmadd_ps(_mm256_cvtph_ps(h(p3.add(c))), lo, a3);
                b3 = _mm256_fmadd_ps(_mm256_cvtph_ps(h(p3.add(c + 8))), hi, b3);
                c += 16;
            }
            out[r] = bias[r] + sum8(_mm256_add_ps(a0, b0));
            out[r + 1] = bias[r + 1] + sum8(_mm256_add_ps(a1, b1));
            out[r + 2] = bias[r + 2] + sum8(_mm256_add_ps(a2, b2));
            out[r + 3] = bias[r + 3] + sum8(_mm256_add_ps(a3, b3));
            r += 4;
        }
    }

    pub fn sigmoids(values: &mut [f32]) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            if super::fast() {
                activate_avx(values, false)
            } else {
                activate_sse(values, false)
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        for v in values {
            *v = super::sigmoid(*v);
        }
    }

    pub fn tanhs(values: &mut [f32]) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            if super::fast() {
                activate_avx(values, true)
            } else {
                activate_sse(values, true)
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        for v in values {
            *v = v.tanh();
        }
    }

    pub fn gates(gx: &[f32], gh: &[f32], h: &mut [f32]) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            if super::fast() {
                gates_avx(gx, gh, h)
            } else {
                gates_sse(gx, gh, h)
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let n = h.len();
            for i in 0..n {
                let z = super::sigmoid(gx[i] + gh[i]);
                let r = super::sigmoid(gx[n + i] + gh[n + i]);
                let c = (gx[2 * n + i] + r * gh[2 * n + i]).tanh();
                h[i] = c + z * (h[i] - c);
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn exp8(x: __m256) -> __m256 {
        let x = _mm256_min_ps(_mm256_max_ps(x, _mm256_set1_ps(-87.0)), _mm256_set1_ps(87.0));
        let fx = _mm256_floor_ps(_mm256_fmadd_ps(x, _mm256_set1_ps(std::f32::consts::LOG2_E), _mm256_set1_ps(0.5)));
        let x = _mm256_fnmadd_ps(fx, _mm256_set1_ps(0.693_359_4), x);
        let x = _mm256_fnmadd_ps(fx, _mm256_set1_ps(-2.121_944_4e-4), x);
        let mut y = _mm256_set1_ps(1.987_569_1e-4);
        y = _mm256_fmadd_ps(y, x, _mm256_set1_ps(1.398_2e-3));
        y = _mm256_fmadd_ps(y, x, _mm256_set1_ps(8.333_452e-3));
        y = _mm256_fmadd_ps(y, x, _mm256_set1_ps(4.166_579_6e-2));
        y = _mm256_fmadd_ps(y, x, _mm256_set1_ps(0.166_666_65));
        y = _mm256_fmadd_ps(y, x, _mm256_set1_ps(0.5));
        y = _mm256_add_ps(_mm256_fmadd_ps(y, _mm256_mul_ps(x, x), x), _mm256_set1_ps(1.0));
        let pow = _mm256_slli_epi32(_mm256_add_epi32(_mm256_cvttps_epi32(fx), _mm256_set1_epi32(127)), 23);
        _mm256_mul_ps(y, _mm256_castsi256_ps(pow))
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn sigmoid8(x: __m256) -> __m256 {
        let one = _mm256_set1_ps(1.0);
        _mm256_div_ps(one, _mm256_add_ps(one, exp8(_mm256_sub_ps(_mm256_setzero_ps(), x))))
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn tanh8(x: __m256) -> __m256 {
        let one = _mm256_set1_ps(1.0);
        let e = exp8(_mm256_add_ps(x, x));
        _mm256_sub_ps(one, _mm256_div_ps(_mm256_set1_ps(2.0), _mm256_add_ps(e, one)))
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn activate_avx(values: &mut [f32], tanh: bool) {
        let n = values.len();
        let p = values.as_mut_ptr();
        let mut i = 0;
        while i + 8 <= n {
            let v = _mm256_loadu_ps(p.add(i));
            _mm256_storeu_ps(p.add(i), if tanh { tanh8(v) } else { sigmoid8(v) });
            i += 8;
        }
        while i < n {
            *p.add(i) = if tanh { (*p.add(i)).tanh() } else { super::sigmoid(*p.add(i)) };
            i += 1;
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn gates_avx(gx: &[f32], gh: &[f32], h: &mut [f32]) {
        let n = h.len();
        assert!(gx.len() >= 3 * n && gh.len() >= 3 * n && n % 8 == 0);
        let (xp, rp, hp) = (gx.as_ptr(), gh.as_ptr(), h.as_mut_ptr());
        let mut i = 0;
        while i < n {
            let z = sigmoid8(_mm256_add_ps(_mm256_loadu_ps(xp.add(i)), _mm256_loadu_ps(rp.add(i))));
            let r = sigmoid8(_mm256_add_ps(_mm256_loadu_ps(xp.add(n + i)), _mm256_loadu_ps(rp.add(n + i))));
            let c = tanh8(_mm256_fmadd_ps(r, _mm256_loadu_ps(rp.add(2 * n + i)), _mm256_loadu_ps(xp.add(2 * n + i))));
            let old = _mm256_loadu_ps(hp.add(i));
            _mm256_storeu_ps(hp.add(i), _mm256_fmadd_ps(z, _mm256_sub_ps(old, c), c));
            i += 8;
        }
    }

    pub fn pointwise(w: &[f32], bias: &[f32], x: &[f32], out: &mut [f32], inputs: usize, width: usize) {
        #[cfg(target_arch = "x86_64")]
        unsafe {
            if super::fast() {
                pointwise_avx(w, bias, x, out, inputs, width)
            } else {
                pointwise_sse(w, bias, x, out, inputs, width)
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            let outputs = out.len() / width;
            for o in 0..outputs {
                let row = &mut out[o * width..(o + 1) * width];
                row.fill(bias.get(o).copied().unwrap_or(0.0));
                for i in 0..inputs {
                    let k = w[o * inputs + i];
                    for (y, v) in row.iter_mut().zip(&x[i * width..(i + 1) * width]) {
                        *y += k * v;
                    }
                }
            }
        }
    }

    pub fn grouped(w: &[f32], x: &[f32], out: &mut [f32], groups: usize) {
        let inputs = x.len() / groups;
        let outputs = out.len() / groups;
        #[cfg(target_arch = "x86_64")]
        unsafe {
            if super::fast() {
                grouped_avx(w, x, out, groups, inputs, outputs)
            } else {
                grouped_sse(w, x, out, groups, inputs, outputs)
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            out.fill(0.0);
            for g in 0..groups {
                for i in 0..inputs {
                    let v = x[g * inputs + i];
                    let row = &w[(g * inputs + i) * outputs..(g * inputs + i + 1) * outputs];
                    for (y, k) in out[g * outputs..(g + 1) * outputs].iter_mut().zip(row) {
                        *y += v * k;
                    }
                }
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn sum8(v: __m256) -> f32 {
        let lo = _mm256_castps256_ps128(v);
        let hi = _mm256_extractf128_ps(v, 1);
        let s = _mm_add_ps(lo, hi);
        let s = _mm_add_ps(s, _mm_movehl_ps(s, s));
        let s = _mm_add_ss(s, _mm_shuffle_ps(s, s, 1));
        _mm_cvtss_f32(s)
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn matvec_avx(w: &[f32], x: &[f32], bias: &[f32], out: &mut [f32]) {
        let cols = x.len();
        let rows = out.len();
        assert!(w.len() >= rows * cols && bias.len() >= rows);
        let main = cols / 8 * 8;
        let xp = x.as_ptr();
        let mut r = 0;
        while r + 4 <= rows {
            let (p0, p1, p2, p3) = (w.as_ptr().add(r * cols), w.as_ptr().add((r + 1) * cols), w.as_ptr().add((r + 2) * cols), w.as_ptr().add((r + 3) * cols));
            let (mut a0, mut a1, mut a2, mut a3) = (_mm256_setzero_ps(), _mm256_setzero_ps(), _mm256_setzero_ps(), _mm256_setzero_ps());
            let mut c = 0;
            while c < main {
                let xv = _mm256_loadu_ps(xp.add(c));
                a0 = _mm256_fmadd_ps(_mm256_loadu_ps(p0.add(c)), xv, a0);
                a1 = _mm256_fmadd_ps(_mm256_loadu_ps(p1.add(c)), xv, a1);
                a2 = _mm256_fmadd_ps(_mm256_loadu_ps(p2.add(c)), xv, a2);
                a3 = _mm256_fmadd_ps(_mm256_loadu_ps(p3.add(c)), xv, a3);
                c += 8;
            }
            let mut s = [sum8(a0), sum8(a1), sum8(a2), sum8(a3)];
            for c in main..cols {
                s[0] += *p0.add(c) * x[c];
                s[1] += *p1.add(c) * x[c];
                s[2] += *p2.add(c) * x[c];
                s[3] += *p3.add(c) * x[c];
            }
            for k in 0..4 {
                out[r + k] = bias[r + k] + s[k];
            }
            r += 4;
        }
        while r < rows {
            out[r] = bias[r] + w[r * cols..(r + 1) * cols].iter().zip(x).map(|(a, b)| a * b).sum::<f32>();
            r += 1;
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn pointwise_avx(w: &[f32], bias: &[f32], x: &[f32], out: &mut [f32], inputs: usize, width: usize) {
        let outputs = out.len() / width;
        assert!(w.len() >= outputs * inputs && x.len() >= inputs * width && width % 8 == 0);
        let (wp, xp, op) = (w.as_ptr(), x.as_ptr(), out.as_mut_ptr());
        let mut o = 0;
        while o < outputs {
            let pair = o + 1 < outputs;
            let (w0, w1) = (wp.add(o * inputs), wp.add((o + pair as usize) * inputs));
            let b0 = _mm256_set1_ps(bias.get(o).copied().unwrap_or(0.0));
            let b1 = _mm256_set1_ps(bias.get(o + 1).copied().unwrap_or(0.0));
            let mut f = 0;
            while f + 16 <= width {
                let (mut a00, mut a01, mut a10, mut a11) = (b0, b0, b1, b1);
                for i in 0..inputs {
                    let x0 = _mm256_loadu_ps(xp.add(i * width + f));
                    let x1 = _mm256_loadu_ps(xp.add(i * width + f + 8));
                    let k0 = _mm256_broadcast_ss(&*w0.add(i));
                    let k1 = _mm256_broadcast_ss(&*w1.add(i));
                    a00 = _mm256_fmadd_ps(k0, x0, a00);
                    a01 = _mm256_fmadd_ps(k0, x1, a01);
                    a10 = _mm256_fmadd_ps(k1, x0, a10);
                    a11 = _mm256_fmadd_ps(k1, x1, a11);
                }
                _mm256_storeu_ps(op.add(o * width + f), a00);
                _mm256_storeu_ps(op.add(o * width + f + 8), a01);
                if pair {
                    _mm256_storeu_ps(op.add((o + 1) * width + f), a10);
                    _mm256_storeu_ps(op.add((o + 1) * width + f + 8), a11);
                }
                f += 16;
            }
            while f < width {
                let (mut a0, mut a1) = (b0, b1);
                for i in 0..inputs {
                    let x0 = _mm256_loadu_ps(xp.add(i * width + f));
                    a0 = _mm256_fmadd_ps(_mm256_broadcast_ss(&*w0.add(i)), x0, a0);
                    a1 = _mm256_fmadd_ps(_mm256_broadcast_ss(&*w1.add(i)), x0, a1);
                }
                _mm256_storeu_ps(op.add(o * width + f), a0);
                if pair {
                    _mm256_storeu_ps(op.add((o + 1) * width + f), a1);
                }
                f += 8;
            }
            o += 2;
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[target_feature(enable = "avx2,fma")]
    unsafe fn grouped_avx(w: &[f32], x: &[f32], out: &mut [f32], groups: usize, inputs: usize, outputs: usize) {
        assert!(w.len() >= groups * inputs * outputs && x.len() >= groups * inputs && out.len() >= groups * outputs && inputs % 2 == 0);
        let main = outputs / 8 * 8;
        let (wp, xp, op) = (w.as_ptr(), x.as_ptr(), out.as_mut_ptr());
        for g in 0..groups {
            let mut h = 0;
            while h < main {
                let two = h + 16 <= main;
                let z = _mm256_setzero_ps();
                let (mut a0, mut a1, mut c0, mut c1) = (z, z, z, z);
                let mut i = 0;
                while i < inputs {
                    let even = (g * inputs + i) * outputs + h;
                    let odd = even + outputs;
                    let ve = _mm256_broadcast_ss(&*xp.add(g * inputs + i));
                    let vo = _mm256_broadcast_ss(&*xp.add(g * inputs + i + 1));
                    a0 = _mm256_fmadd_ps(ve, _mm256_loadu_ps(wp.add(even)), a0);
                    a1 = _mm256_fmadd_ps(vo, _mm256_loadu_ps(wp.add(odd)), a1);
                    if two {
                        c0 = _mm256_fmadd_ps(ve, _mm256_loadu_ps(wp.add(even + 8)), c0);
                        c1 = _mm256_fmadd_ps(vo, _mm256_loadu_ps(wp.add(odd + 8)), c1);
                    }
                    i += 2;
                }
                _mm256_storeu_ps(op.add(g * outputs + h), _mm256_add_ps(a0, a1));
                if two {
                    _mm256_storeu_ps(op.add(g * outputs + h + 8), _mm256_add_ps(c0, c1));
                    h += 16;
                } else {
                    h += 8;
                }
            }
            for h in main..outputs {
                let mut a = 0.0;
                for i in 0..inputs {
                    a += *xp.add(g * inputs + i) * *wp.add((g * inputs + i) * outputs + h);
                }
                *op.add(g * outputs + h) = a;
            }
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    unsafe fn sum4(v: __m128) -> f32 {
        let s = _mm_add_ps(v, _mm_movehl_ps(v, v));
        _mm_cvtss_f32(_mm_add_ss(s, _mm_shuffle_ps(s, s, 1)))
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    unsafe fn sum4i(v: __m128i) -> i32 {
        let s = _mm_add_epi32(v, _mm_shuffle_epi32(v, 0b01_00_11_10));
        _mm_cvtsi128_si32(_mm_add_epi32(s, _mm_shuffle_epi32(s, 0b10_11_00_01)))
    }

    #[cfg(target_arch = "x86_64")]
    unsafe fn axpy_sse(y: &mut [f32], a: f32, x: &[f32]) {
        let n = y.len().min(x.len());
        let (yp, xp) = (y.as_mut_ptr(), x.as_ptr());
        let k = _mm_set1_ps(a);
        let mut i = 0;
        while i + 4 <= n {
            _mm_storeu_ps(yp.add(i), _mm_add_ps(_mm_loadu_ps(yp.add(i)), _mm_mul_ps(k, _mm_loadu_ps(xp.add(i)))));
            i += 4;
        }
        while i < n {
            *yp.add(i) += a * *xp.add(i);
            i += 1;
        }
    }

    #[cfg(target_arch = "x86_64")]
    unsafe fn matvec_sse(w: &[f32], x: &[f32], bias: &[f32], out: &mut [f32]) {
        let cols = x.len();
        let rows = out.len();
        assert!(cols % 8 == 0 && w.len() >= rows * cols && bias.len() >= rows && rows % 4 == 0);
        let xp = x.as_ptr();
        let mut r = 0;
        while r < rows {
            let p0 = w.as_ptr().add(r * cols);
            let (p1, p2, p3) = (p0.add(cols), p0.add(2 * cols), p0.add(3 * cols));
            let z = _mm_setzero_ps();
            let (mut a0, mut b0, mut a1, mut b1, mut a2, mut b2, mut a3, mut b3) = (z, z, z, z, z, z, z, z);
            let mut c = 0;
            while c < cols {
                let lo = _mm_loadu_ps(xp.add(c));
                let hi = _mm_loadu_ps(xp.add(c + 4));
                a0 = _mm_add_ps(a0, _mm_mul_ps(_mm_loadu_ps(p0.add(c)), lo));
                b0 = _mm_add_ps(b0, _mm_mul_ps(_mm_loadu_ps(p0.add(c + 4)), hi));
                a1 = _mm_add_ps(a1, _mm_mul_ps(_mm_loadu_ps(p1.add(c)), lo));
                b1 = _mm_add_ps(b1, _mm_mul_ps(_mm_loadu_ps(p1.add(c + 4)), hi));
                a2 = _mm_add_ps(a2, _mm_mul_ps(_mm_loadu_ps(p2.add(c)), lo));
                b2 = _mm_add_ps(b2, _mm_mul_ps(_mm_loadu_ps(p2.add(c + 4)), hi));
                a3 = _mm_add_ps(a3, _mm_mul_ps(_mm_loadu_ps(p3.add(c)), lo));
                b3 = _mm_add_ps(b3, _mm_mul_ps(_mm_loadu_ps(p3.add(c + 4)), hi));
                c += 8;
            }
            out[r] = bias[r] + sum4(_mm_add_ps(a0, b0));
            out[r + 1] = bias[r + 1] + sum4(_mm_add_ps(a1, b1));
            out[r + 2] = bias[r + 2] + sum4(_mm_add_ps(a2, b2));
            out[r + 3] = bias[r + 3] + sum4(_mm_add_ps(a3, b3));
            r += 4;
        }
    }

    #[cfg(target_arch = "x86_64")]
    unsafe fn matvec_byte_sse(w: &[i8], scales: &[f32], x: &[f32], bias: &[f32], out: &mut [f32]) {
        let cols = x.len();
        let rows = out.len();
        assert!(cols == super::HID && w.len() >= rows * cols && bias.len() >= rows && scales.len() >= rows && rows % 4 == 0);
        let xr = x.as_ptr();
        let mut top = _mm_setzero_ps();
        let bits = _mm_castsi128_ps(_mm_set1_epi32(0x7fff_ffff));
        let mut c = 0;
        while c < cols {
            top = _mm_max_ps(top, _mm_and_ps(_mm_loadu_ps(xr.add(c)), bits));
            c += 4;
        }
        let top = _mm_max_ps(top, _mm_movehl_ps(top, top));
        let top = _mm_cvtss_f32(_mm_max_ss(top, _mm_shuffle_ps(top, top, 1)));
        let up = if top > 0.0 { 32_767.0 / top } else { 1.0 };
        let k = _mm_set1_ps(up);
        let mut words = [0i16; super::HID];
        let wp = words.as_mut_ptr();
        let mut c = 0;
        while c < cols {
            let lo = _mm_cvtps_epi32(_mm_mul_ps(_mm_loadu_ps(xr.add(c)), k));
            let hi = _mm_cvtps_epi32(_mm_mul_ps(_mm_loadu_ps(xr.add(c + 4)), k));
            _mm_storeu_si128(wp.add(c) as *mut __m128i, _mm_packs_epi32(lo, hi));
            c += 8;
        }
        let down = 1.0 / up;
        let xp = words.as_ptr();
        let mut r = 0;
        while r < rows {
            let p0 = w.as_ptr().add(r * cols);
            let (p1, p2, p3) = (p0.add(cols), p0.add(2 * cols), p0.add(3 * cols));
            let z = _mm_setzero_si128();
            let (mut a0, mut a1, mut a2, mut a3) = (z, z, z, z);
            let mut c = 0;
            while c < cols {
                let lo = _mm_loadu_si128(xp.add(c) as *const __m128i);
                let hi = _mm_loadu_si128(xp.add(c + 8) as *const __m128i);
                let step = |a: __m128i, p: *const i8| {
                    let q = _mm_loadu_si128(p.add(c) as *const __m128i);
                    let low = _mm_madd_epi16(_mm_srai_epi16(_mm_unpacklo_epi8(q, q), 8), lo);
                    let high = _mm_madd_epi16(_mm_srai_epi16(_mm_unpackhi_epi8(q, q), 8), hi);
                    _mm_add_epi32(a, _mm_add_epi32(low, high))
                };
                a0 = step(a0, p0);
                a1 = step(a1, p1);
                a2 = step(a2, p2);
                a3 = step(a3, p3);
                c += 16;
            }
            out[r] = bias[r] + scales[r] * down * sum4i(a0) as f32;
            out[r + 1] = bias[r + 1] + scales[r + 1] * down * sum4i(a1) as f32;
            out[r + 2] = bias[r + 2] + scales[r + 2] * down * sum4i(a2) as f32;
            out[r + 3] = bias[r + 3] + scales[r + 3] * down * sum4i(a3) as f32;
            r += 4;
        }
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    unsafe fn exp4(x: __m128) -> __m128 {
        let x = _mm_min_ps(_mm_max_ps(x, _mm_set1_ps(-87.0)), _mm_set1_ps(87.0));
        let n = _mm_cvtps_epi32(_mm_mul_ps(x, _mm_set1_ps(std::f32::consts::LOG2_E)));
        let fx = _mm_cvtepi32_ps(n);
        let x = _mm_sub_ps(x, _mm_mul_ps(fx, _mm_set1_ps(0.693_359_4)));
        let x = _mm_sub_ps(x, _mm_mul_ps(fx, _mm_set1_ps(-2.121_944_4e-4)));
        let mut y = _mm_set1_ps(1.987_569_1e-4);
        y = _mm_add_ps(_mm_mul_ps(y, x), _mm_set1_ps(1.398_2e-3));
        y = _mm_add_ps(_mm_mul_ps(y, x), _mm_set1_ps(8.333_452e-3));
        y = _mm_add_ps(_mm_mul_ps(y, x), _mm_set1_ps(4.166_579_6e-2));
        y = _mm_add_ps(_mm_mul_ps(y, x), _mm_set1_ps(0.166_666_65));
        y = _mm_add_ps(_mm_mul_ps(y, x), _mm_set1_ps(0.5));
        y = _mm_add_ps(_mm_add_ps(_mm_mul_ps(y, _mm_mul_ps(x, x)), x), _mm_set1_ps(1.0));
        let pow = _mm_slli_epi32(_mm_add_epi32(n, _mm_set1_epi32(127)), 23);
        _mm_mul_ps(y, _mm_castsi128_ps(pow))
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    unsafe fn sigmoid4(x: __m128) -> __m128 {
        let one = _mm_set1_ps(1.0);
        _mm_div_ps(one, _mm_add_ps(one, exp4(_mm_sub_ps(_mm_setzero_ps(), x))))
    }

    #[cfg(target_arch = "x86_64")]
    #[inline]
    unsafe fn tanh4(x: __m128) -> __m128 {
        let one = _mm_set1_ps(1.0);
        _mm_sub_ps(one, _mm_div_ps(_mm_set1_ps(2.0), _mm_add_ps(exp4(_mm_add_ps(x, x)), one)))
    }

    #[cfg(target_arch = "x86_64")]
    unsafe fn activate_sse(values: &mut [f32], tanh: bool) {
        let n = values.len();
        let p = values.as_mut_ptr();
        let mut i = 0;
        while i + 4 <= n {
            let v = _mm_loadu_ps(p.add(i));
            _mm_storeu_ps(p.add(i), if tanh { tanh4(v) } else { sigmoid4(v) });
            i += 4;
        }
        while i < n {
            *p.add(i) = if tanh { (*p.add(i)).tanh() } else { super::sigmoid(*p.add(i)) };
            i += 1;
        }
    }

    #[cfg(target_arch = "x86_64")]
    unsafe fn gates_sse(gx: &[f32], gh: &[f32], h: &mut [f32]) {
        let n = h.len();
        assert!(gx.len() >= 3 * n && gh.len() >= 3 * n && n % 4 == 0);
        let (xp, rp, hp) = (gx.as_ptr(), gh.as_ptr(), h.as_mut_ptr());
        let mut i = 0;
        while i < n {
            let z = sigmoid4(_mm_add_ps(_mm_loadu_ps(xp.add(i)), _mm_loadu_ps(rp.add(i))));
            let r = sigmoid4(_mm_add_ps(_mm_loadu_ps(xp.add(n + i)), _mm_loadu_ps(rp.add(n + i))));
            let c = tanh4(_mm_add_ps(_mm_loadu_ps(xp.add(2 * n + i)), _mm_mul_ps(r, _mm_loadu_ps(rp.add(2 * n + i)))));
            let old = _mm_loadu_ps(hp.add(i));
            _mm_storeu_ps(hp.add(i), _mm_add_ps(c, _mm_mul_ps(z, _mm_sub_ps(old, c))));
            i += 4;
        }
    }

    #[cfg(target_arch = "x86_64")]
    unsafe fn pointwise_sse(w: &[f32], bias: &[f32], x: &[f32], out: &mut [f32], inputs: usize, width: usize) {
        let outputs = out.len() / width;
        assert!(w.len() >= outputs * inputs && x.len() >= inputs * width && width % 8 == 0);
        let (wp, xp, op) = (w.as_ptr(), x.as_ptr(), out.as_mut_ptr());
        let mut o = 0;
        while o < outputs {
            let pair = o + 1 < outputs;
            let (w0, w1) = (wp.add(o * inputs), wp.add((o + pair as usize) * inputs));
            let b0 = _mm_set1_ps(bias.get(o).copied().unwrap_or(0.0));
            let b1 = _mm_set1_ps(bias.get(o + 1).copied().unwrap_or(0.0));
            let mut f = 0;
            while f < width {
                let (mut a00, mut a01, mut a10, mut a11) = (b0, b0, b1, b1);
                for i in 0..inputs {
                    let x0 = _mm_loadu_ps(xp.add(i * width + f));
                    let x1 = _mm_loadu_ps(xp.add(i * width + f + 4));
                    let k0 = _mm_set1_ps(*w0.add(i));
                    let k1 = _mm_set1_ps(*w1.add(i));
                    a00 = _mm_add_ps(a00, _mm_mul_ps(k0, x0));
                    a01 = _mm_add_ps(a01, _mm_mul_ps(k0, x1));
                    a10 = _mm_add_ps(a10, _mm_mul_ps(k1, x0));
                    a11 = _mm_add_ps(a11, _mm_mul_ps(k1, x1));
                }
                _mm_storeu_ps(op.add(o * width + f), a00);
                _mm_storeu_ps(op.add(o * width + f + 4), a01);
                if pair {
                    _mm_storeu_ps(op.add((o + 1) * width + f), a10);
                    _mm_storeu_ps(op.add((o + 1) * width + f + 4), a11);
                }
                f += 8;
            }
            o += 2;
        }
    }

    #[cfg(target_arch = "x86_64")]
    unsafe fn grouped_sse(w: &[f32], x: &[f32], out: &mut [f32], groups: usize, inputs: usize, outputs: usize) {
        assert!(w.len() >= groups * inputs * outputs && x.len() >= groups * inputs && out.len() >= groups * outputs && inputs % 2 == 0);
        let main = outputs / 4 * 4;
        let (wp, xp, op) = (w.as_ptr(), x.as_ptr(), out.as_mut_ptr());
        for g in 0..groups {
            let mut h = 0;
            while h < main {
                let two = h + 8 <= main;
                let z = _mm_setzero_ps();
                let (mut a0, mut a1, mut c0, mut c1) = (z, z, z, z);
                let mut i = 0;
                while i < inputs {
                    let even = (g * inputs + i) * outputs + h;
                    let odd = even + outputs;
                    let ve = _mm_set1_ps(*xp.add(g * inputs + i));
                    let vo = _mm_set1_ps(*xp.add(g * inputs + i + 1));
                    a0 = _mm_add_ps(a0, _mm_mul_ps(ve, _mm_loadu_ps(wp.add(even))));
                    a1 = _mm_add_ps(a1, _mm_mul_ps(vo, _mm_loadu_ps(wp.add(odd))));
                    if two {
                        c0 = _mm_add_ps(c0, _mm_mul_ps(ve, _mm_loadu_ps(wp.add(even + 4))));
                        c1 = _mm_add_ps(c1, _mm_mul_ps(vo, _mm_loadu_ps(wp.add(odd + 4))));
                    }
                    i += 2;
                }
                _mm_storeu_ps(op.add(g * outputs + h), _mm_add_ps(a0, a1));
                if two {
                    _mm_storeu_ps(op.add(g * outputs + h + 4), _mm_add_ps(c0, c1));
                    h += 8;
                } else {
                    h += 4;
                }
            }
            for h in main..outputs {
                let mut a = 0.0;
                for i in 0..inputs {
                    a += *xp.add(g * inputs + i) * *wp.add((g * inputs + i) * outputs + h);
                }
                *op.add(g * outputs + h) = a;
            }
        }
    }

    #[cfg(all(test, target_arch = "x86_64"))]
    mod sse {
        use super::*;
        use crate::profundo::{quantize, sigmoid, Mat, HID};

        fn noise(n: usize, seed: u32) -> Vec<f32> {
            let mut s = seed | 1;
            (0..n)
                .map(|_| {
                    s ^= s << 13;
                    s ^= s >> 17;
                    s ^= s << 5;
                    s as f32 / u32::MAX as f32 * 2.0 - 1.0
                })
                .collect()
        }

        fn close(a: &[f32], b: &[f32], tolerance: f32) {
            assert_eq!(a.len(), b.len());
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                assert!((x - y).abs() <= tolerance, "{i}: {x} contra {y}");
            }
        }

        fn plain(w: &[f32], x: &[f32], bias: &[f32]) -> Vec<f32> {
            w.chunks_exact(x.len()).enumerate().map(|(r, row)| bias[r] + row.iter().zip(x).map(|(a, b)| a * b).sum::<f32>()).collect()
        }

        #[test]
        fn the_float_product_matches_plain_math() {
            let (w, x, bias) = (noise(768 * HID, 1), noise(HID, 2), noise(768, 3));
            let mut out = vec![0.0; 768];
            unsafe { matvec_sse(&w, &x, &bias, &mut out) };
            close(&out, &plain(&w, &x, &bias), 1e-3);
        }

        #[test]
        fn the_byte_product_matches_plain_math_at_any_scale() {
            let (w, bias) = (noise(768 * HID, 4), noise(768, 5));
            let Mat::Byte(q, scales) = quantize(&w, HID) else { panic!() };
            let exact: Vec<f32> = q.iter().map(|v| *v as f32).collect();
            for scale in [1.0f32, 0.001, 40.0] {
                let x: Vec<f32> = noise(HID, 6).iter().map(|v| v * scale).collect();
                let mut out = vec![0.0; 768];
                unsafe { matvec_byte_sse(&q, &scales, &x, &bias, &mut out) };
                let want: Vec<f32> = exact.chunks_exact(HID).enumerate().map(|(r, row)| bias[r] + scales[r] * row.iter().zip(&x).map(|(a, b)| a * b).sum::<f32>()).collect();
                close(&out, &want, 2e-3 * scale.max(1.0));
            }
            let mut out = vec![9.0; 768];
            unsafe { matvec_byte_sse(&q, &scales, &vec![0.0; HID], &bias, &mut out) };
            close(&out, &bias, 0.0);
        }

        #[test]
        fn the_byte_product_never_overflows() {
            let w = vec![127i8; 4 * HID];
            let scales = vec![1.0; 4];
            let x = vec![1.0f32; HID];
            let mut out = vec![0.0; 4];
            unsafe { matvec_byte_sse(&w, &scales, &x, &[0.0; 4], &mut out) };
            close(&out, &[127.0 * HID as f32; 4], 0.01);
        }

        #[test]
        fn pointwise_matches_plain_math() {
            for (inputs, outputs, width, with_bias) in [(64, 64, 96, true), (64, 64, 8, true), (160, 5, 96, false), (10, 10, 96, true)] {
                let (w, x, bias) = (noise(outputs * inputs, 7), noise(inputs * width, 8), noise(outputs, 9));
                let bias: &[f32] = if with_bias { &bias } else { &[] };
                let mut out = vec![0.0; outputs * width];
                unsafe { pointwise_sse(&w, bias, &x, &mut out, inputs, width) };
                let mut want = vec![0.0; outputs * width];
                for o in 0..outputs {
                    for f in 0..width {
                        want[o * width + f] = bias.get(o).copied().unwrap_or(0.0) + (0..inputs).map(|i| w[o * inputs + i] * x[i * width + f]).sum::<f32>();
                    }
                }
                close(&out, &want, 1e-3);
            }
        }

        #[test]
        fn grouped_matches_plain_math() {
            for (groups, inputs, outputs) in [(32, 96, 16), (16, 32, 16), (16, 16, 32), (8, 64, 32), (16, 16, 60)] {
                let (w, x) = (noise(groups * inputs * outputs, 10), noise(groups * inputs, 11));
                let mut out = vec![0.0; groups * outputs];
                unsafe { grouped_sse(&w, &x, &mut out, groups, inputs, outputs) };
                let mut want = vec![0.0; groups * outputs];
                for g in 0..groups {
                    for h in 0..outputs {
                        want[g * outputs + h] = (0..inputs).map(|i| x[g * inputs + i] * w[(g * inputs + i) * outputs + h]).sum::<f32>();
                    }
                }
                close(&out, &want, 1e-3);
            }
        }

        #[test]
        fn axpy_and_the_activations_match_plain_math() {
            let (x, start) = (noise(37, 12), noise(37, 13));
            let mut y = start.clone();
            unsafe { axpy_sse(&mut y, 0.7, &x) };
            let want: Vec<f32> = start.iter().zip(&x).map(|(a, b)| a + 0.7 * b).collect();
            close(&y, &want, 1e-6);
            let wide: Vec<f32> = noise(4003, 14).iter().map(|v| v * 30.0).collect();
            let mut s = wide.clone();
            unsafe { activate_sse(&mut s, false) };
            close(&s, &wide.iter().map(|v| sigmoid(*v)).collect::<Vec<_>>(), 2e-6);
            let mut t = wide.clone();
            unsafe { activate_sse(&mut t, true) };
            close(&t, &wide.iter().map(|v| v.tanh()).collect::<Vec<_>>(), 2e-6);
        }

        #[test]
        fn the_gates_match_plain_math() {
            let (gx, gh, start) = (noise(3 * HID, 15), noise(3 * HID, 16), noise(HID, 17));
            let mut h = start.clone();
            unsafe { gates_sse(&gx, &gh, &mut h) };
            let n = HID;
            let want: Vec<f32> = (0..n)
                .map(|i| {
                    let z = sigmoid(gx[i] + gh[i]);
                    let r = sigmoid(gx[n + i] + gh[n + i]);
                    let c = (gx[2 * n + i] + r * gh[2 * n + i]).tanh();
                    c + z * (start[i] - c)
                })
                .collect();
            close(&h, &want, 2e-5);
        }
    }
}

struct Stage {
    p: usize,
    m: usize,
    s: usize,
    twiddles: Vec<(f32, f32)>,
}

struct Fft {
    size: usize,
    stages: Vec<Stage>,
    turn: Vec<(f32, f32)>,
    zr: Vec<f32>,
    zi: Vec<f32>,
    sr: Vec<f32>,
    si: Vec<f32>,
}

impl Fft {
    fn new(real: usize) -> Fft {
        let size = real / 2;
        let mut left = size;
        let mut radices = Vec::new();
        for p in [4, 2, 3, 5] {
            while left % p == 0 {
                radices.push(p);
                left /= p;
            }
        }
        assert_eq!(left, 1, "la fft solo sabe de 2, 3 y 5");
        let angle = |a: usize, b: usize| {
            let t = -2.0 * std::f64::consts::PI * a as f64 / b as f64;
            (t.cos() as f32, t.sin() as f32)
        };
        let mut stages = Vec::new();
        let (mut n, mut s) = (size, 1);
        for p in radices {
            let m = n / p;
            let twiddles = (0..m).flat_map(|j| (0..p).map(move |k| (j * k, n))).map(|(a, b)| angle(a, b)).collect();
            stages.push(Stage { p, m, s, twiddles });
            n = m;
            s *= p;
        }
        let zeros = || vec![0.0f32; size];
        Fft { size, stages, turn: (0..=size).map(|k| angle(k, real)).collect(), zr: zeros(), zi: zeros(), sr: zeros(), si: zeros() }
    }

    fn complex(&mut self) {
        let mut flip = false;
        for st in &self.stages {
            let (xr, xi, yr, yi) = if flip {
                (self.sr.as_ptr(), self.si.as_ptr(), self.zr.as_mut_ptr(), self.zi.as_mut_ptr())
            } else {
                (self.zr.as_ptr(), self.zi.as_ptr(), self.sr.as_mut_ptr(), self.si.as_mut_ptr())
            };
            unsafe { stage(st, xr, xi, yr, yi) };
            flip = !flip;
        }
        if flip {
            self.zr.copy_from_slice(&self.sr);
            self.zi.copy_from_slice(&self.si);
        }
    }

    fn forward(&mut self, x: &[f32], re: &mut [f32], im: &mut [f32], scale: f32) {
        let m = self.size;
        for n in 0..m {
            self.zr[n] = x[2 * n];
            self.zi[n] = x[2 * n + 1];
        }
        self.complex();
        for k in 0..=m {
            let (ar, ai) = (self.zr[k % m], self.zi[k % m]);
            let (br, bi) = (self.zr[(m - k) % m], -self.zi[(m - k) % m]);
            let (er, ei) = ((ar + br) * 0.5, (ai + bi) * 0.5);
            let (or, oi) = ((ai - bi) * 0.5, -(ar - br) * 0.5);
            let (wr, wi) = self.turn[k];
            re[k] = (er + wr * or - wi * oi) * scale;
            im[k] = (ei + wr * oi + wi * or) * scale;
        }
    }

    fn inverse(&mut self, re: &[f32], im: &[f32], x: &mut [f32]) {
        let m = self.size;
        let imag = |k: usize| if k == 0 || k == m { 0.0 } else { im[k] };
        for k in 0..m {
            let (ar, ai) = (re[k], imag(k));
            let (br, bi) = (re[m - k], -imag(m - k));
            let (dr, di) = (ar - br, ai - bi);
            let (wr, wi) = self.turn[k];
            let (qr, qi) = (dr * wr + di * wi, di * wr - dr * wi);
            self.zr[k] = ar + br - qi;
            self.zi[k] = -(ai + bi + qr);
        }
        self.complex();
        for n in 0..m {
            x[2 * n] = self.zr[n];
            x[2 * n + 1] = -self.zi[n];
        }
    }
}

unsafe fn stage(st: &Stage, xr: *const f32, xi: *const f32, yr: *mut f32, yi: *mut f32) {
    let (p, m, s) = (st.p, st.m, st.s);
    let tw = st.twiddles.as_ptr();
    let (c3, s3) = (-0.5f32, -(0.75f32).sqrt());
    let (c51, s51) = ((2.0 * PI / 5.0).cos(), -(2.0 * PI / 5.0).sin());
    let (c52, s52) = ((4.0 * PI / 5.0).cos(), -(4.0 * PI / 5.0).sin());
    for j in 0..m {
        for q in 0..s {
            let at = |r: usize| q + s * (j + r * m);
            let put = |k: usize, vr: f32, vi: f32| {
                let (wr, wi) = *tw.add(j * p + k);
                let to = q + s * (p * j + k);
                *yr.add(to) = vr * wr - vi * wi;
                *yi.add(to) = vr * wi + vi * wr;
            };
            match p {
                2 => {
                    let (ar, ai, br, bi) = (*xr.add(at(0)), *xi.add(at(0)), *xr.add(at(1)), *xi.add(at(1)));
                    put(0, ar + br, ai + bi);
                    put(1, ar - br, ai - bi);
                }
                3 => {
                    let (ar, ai) = (*xr.add(at(0)), *xi.add(at(0)));
                    let (br, bi, cr, ci) = (*xr.add(at(1)), *xi.add(at(1)), *xr.add(at(2)), *xi.add(at(2)));
                    let (sr, si, dr, di) = (br + cr, bi + ci, br - cr, bi - ci);
                    let (mr, mi) = (ar + c3 * sr, ai + c3 * si);
                    put(0, ar + sr, ai + si);
                    put(1, mr - s3 * di, mi + s3 * dr);
                    put(2, mr + s3 * di, mi - s3 * dr);
                }
                4 => {
                    let (ar, ai, br, bi) = (*xr.add(at(0)), *xi.add(at(0)), *xr.add(at(1)), *xi.add(at(1)));
                    let (cr, ci, dr, di) = (*xr.add(at(2)), *xi.add(at(2)), *xr.add(at(3)), *xi.add(at(3)));
                    let (t0r, t0i, t1r, t1i) = (ar + cr, ai + ci, ar - cr, ai - ci);
                    let (t2r, t2i, t3r, t3i) = (br + dr, bi + di, br - dr, bi - di);
                    put(0, t0r + t2r, t0i + t2i);
                    put(1, t1r + t3i, t1i - t3r);
                    put(2, t0r - t2r, t0i - t2i);
                    put(3, t1r - t3i, t1i + t3r);
                }
                _ => {
                    let a = [(*xr.add(at(0)), *xi.add(at(0))), (*xr.add(at(1)), *xi.add(at(1))), (*xr.add(at(2)), *xi.add(at(2))), (*xr.add(at(3)), *xi.add(at(3))), (*xr.add(at(4)), *xi.add(at(4)))];
                    let (s1r, s1i, d1r, d1i) = (a[1].0 + a[4].0, a[1].1 + a[4].1, a[1].0 - a[4].0, a[1].1 - a[4].1);
                    let (s2r, s2i, d2r, d2i) = (a[2].0 + a[3].0, a[2].1 + a[3].1, a[2].0 - a[3].0, a[2].1 - a[3].1);
                    put(0, a[0].0 + s1r + s2r, a[0].1 + s1i + s2i);
                    let (m1r, m1i) = (a[0].0 + c51 * s1r + c52 * s2r, a[0].1 + c51 * s1i + c52 * s2i);
                    let (m2r, m2i) = (a[0].0 + c52 * s1r + c51 * s2r, a[0].1 + c52 * s1i + c51 * s2i);
                    let (n1r, n1i) = (-(s51 * d1i + s52 * d2i), s51 * d1r + s52 * d2r);
                    let (n2r, n2i) = (-(s52 * d1i - s51 * d2i), s52 * d1r - s51 * d2r);
                    put(1, m1r + n1r, m1i + n1i);
                    put(4, m1r - n1r, m1i - n1i);
                    put(2, m2r + n2r, m2i + n2i);
                    put(3, m2r - n2r, m2i - n2i);
                }
            }
        }
    }
}

fn bands() -> [usize; ERB] {
    let to_erb = |f: f32| 9.265 * (f / (24.7 * 9.265)).ln_1p();
    let to_freq = |e: f32| 24.7 * 9.265 * ((e / 9.265).exp() - 1.0);
    let width = RATE as f32 / FFT as f32;
    let (low, high) = (to_erb(0.0), to_erb((RATE / 2) as f32));
    let step = (high - low) / ERB as f32;
    let mut out = [0usize; ERB];
    let (mut previous, mut over) = (0i64, 0i64);
    for i in 1..=ERB {
        let edge = (to_freq(low + i as f32 * step) / width).round() as i64;
        let mut count = edge - previous - over;
        if count < 2 {
            over = 2 - count;
            count = 2;
        } else {
            over = 0;
        }
        out[i - 1] = count as usize;
        previous = edge;
    }
    out[ERB - 1] += 1;
    let total: usize = out.iter().sum();
    if total > FREQS {
        out[ERB - 1] -= total - FREQS;
    }
    out
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn relu(values: &mut [f32]) {
    for v in values {
        *v = v.max(0.0);
    }
}

fn product(m: &Mat, x: &[f32], bias: &[f32], out: &mut [f32]) {
    match m {
        Mat::Byte(w, scales) => kernel::matvec_byte(w, scales, x, bias, out),
        Mat::Half(w) => kernel::matvec_half(w, x, bias, out),
        Mat::Full(w) => kernel::matvec(w, x, bias, out),
    }
}

fn gru(cell: &Gru, x: &[f32], h: &mut [f32], gx: &mut [f32], gh: &mut [f32]) {
    product(&cell.w, x, &cell.b[..3 * HID], gx);
    product(&cell.r, h, &cell.b[3 * HID..], gh);
    kernel::gates(gx, gh, h);
}

fn depthwise(w: &[f32], x: &[f32], out: &mut [f32], width: usize, stride: usize) {
    let outw = out.len() / CH;
    let mut row = [0.0f32; DF + 2];
    for c in 0..CH {
        let (k0, k1, k2) = (w[c * 3], w[c * 3 + 1], w[c * 3 + 2]);
        row[1..=width].copy_from_slice(&x[c * width..(c + 1) * width]);
        row[width + 1] = 0.0;
        for (j, o) in out[c * outw..(c + 1) * outw].iter_mut().enumerate() {
            let at = j * stride;
            *o = k0 * row[at] + k1 * row[at + 1] + k2 * row[at + 2];
        }
    }
}

fn transposed(w: &[f32], x: &[f32], out: &mut [f32], width: usize) {
    let outw = width * 2;
    let mut row = [0.0f32; 2 * 32 + 2];
    for c in 0..CH {
        let (k0, k1, k2) = (w[c * 3], w[c * 3 + 1], w[c * 3 + 2]);
        row[..outw + 2].fill(0.0);
        for (i, &v) in x[c * width..(c + 1) * width].iter().enumerate() {
            row[2 * i] += v * k0;
            row[2 * i + 1] += v * k1;
            row[2 * i + 2] += v * k2;
        }
        out[c * outw..(c + 1) * outw].copy_from_slice(&row[1..=outw]);
    }
}

fn padded(rows: &[&[f32]], out: &mut [f32]) {
    let width = rows[0].len();
    for (r, row) in rows.iter().enumerate() {
        let to = &mut out[r * (width + 2)..(r + 1) * (width + 2)];
        to[0] = 0.0;
        to[1..=width].copy_from_slice(row);
        to[width + 1] = 0.0;
    }
}

fn conv3x3(w: &[f32], pad: &[f32], groups: usize, width: usize, out: &mut [f32], bias: Option<&[f32]>) {
    let span = width + 2;
    for o in 0..CH {
        let g = o * groups / CH;
        let row = &mut out[o * width..(o + 1) * width];
        row.fill(bias.map_or(0.0, |b| b[o]));
        for kt in 0..3 {
            let base = (g * 3 + kt) * span;
            for kf in 0..3 {
                kernel::axpy(row, w[o * 9 + kt * 3 + kf], &pad[base + kf..base + kf + width]);
            }
        }
    }
}

fn flatten(x: &[f32], width: usize, out: &mut [f32]) {
    for c in 0..CH {
        for f in 0..width {
            out[f * CH + c] = x[c * width + f];
        }
    }
}

pub struct Profundo {
    net: Arc<Net>,
    fft: Fft,
    window: Vec<f32>,
    bands: [usize; ERB],
    analysis: Vec<f32>,
    synthesis: Vec<f32>,
    mean_norm: [f32; ERB],
    unit_norm: [f32; DF],
    ring_re: Vec<Vec<f32>>,
    ring_im: Vec<Vec<f32>>,
    erb_hist: [[f32; ERB]; 3],
    spec_hist: [[f32; 2 * DF]; 3],
    h_enc: Vec<f32>,
    h_erb: [Vec<f32>; 2],
    h_df: [Vec<f32>; 2],
    convp_hist: [Vec<f32>; 2],
    #[cfg_attr(not(test), allow(dead_code))]
    pending: Vec<f32>,
    quiet: u32,
    limit: Option<f32>,
    post: Option<f32>,
    thresholds: (f32, f32, f32),
    s: Scratch,
}

struct Scratch {
    e0: Vec<f32>,
    e1: Vec<f32>,
    e2: Vec<f32>,
    e3: Vec<f32>,
    c0: Vec<f32>,
    c1: Vec<f32>,
    a: Vec<f32>,
    b: Vec<f32>,
    d: Vec<f32>,
    flat: Vec<f32>,
    emb: Vec<f32>,
    v256: Vec<f32>,
    w256: Vec<f32>,
    gx: Vec<f32>,
    gh: Vec<f32>,
    coefs: Vec<f32>,
    p10: Vec<f32>,
    q10: Vec<f32>,
    mask: [f32; ERB],
    out_re: Vec<f32>,
    out_im: Vec<f32>,
    frame: Vec<f32>,
}

impl Profundo {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn new(level: Level) -> Profundo {
        Profundo::with(level, false)
    }

    pub fn with(level: Level, precise: bool) -> Profundo {
        let half = FFT / 2;
        let window = (0..FFT)
            .map(|i| {
                let s = (0.5 * std::f64::consts::PI * (i as f64 + 0.5) / half as f64).sin();
                (0.5 * std::f64::consts::PI * s * s).sin() as f32
            })
            .collect();
        let mut mean_norm = [0.0; ERB];
        for (i, v) in mean_norm.iter_mut().enumerate() {
            *v = -60.0 + i as f32 * (-30.0 / (ERB - 1) as f32);
        }
        let mut unit_norm = [0.0; DF];
        for (i, v) in unit_norm.iter_mut().enumerate() {
            *v = 0.001 + i as f32 * ((0.0001 - 0.001) / (DF - 1) as f32);
        }
        let zeros = |n: usize| vec![0.0f32; n];
        let mut engine = Profundo {
            net: shared(precise),
            fft: Fft::new(FFT),
            window,
            bands: bands(),
            analysis: zeros(HOP),
            synthesis: zeros(HOP),
            mean_norm,
            unit_norm,
            ring_re: vec![zeros(FREQS); ORDER],
            ring_im: vec![zeros(FREQS); ORDER],
            erb_hist: [[0.0; ERB]; 3],
            spec_hist: [[0.0; 2 * DF]; 3],
            h_enc: zeros(HID),
            h_erb: [zeros(HID), zeros(HID)],
            h_df: [zeros(HID), zeros(HID)],
            convp_hist: [zeros(160 * DF), zeros(160 * DF)],
            pending: Vec::with_capacity(HOP * 8),
            quiet: 0,
            limit: None,
            post: None,
            thresholds: (-15.0, 35.0, 35.0),
            s: Scratch {
                e0: zeros(CH * 32),
                e1: zeros(CH * 16),
                e2: zeros(CH * 8),
                e3: zeros(CH * 8),
                c0: zeros(CH * DF),
                c1: zeros(CH * 48),
                a: zeros(CH * DF),
                b: zeros(CH * DF),
                d: zeros(CH * 32),
                flat: zeros(CH * 48),
                emb: zeros(512),
                v256: zeros(HID),
                w256: zeros(HID),
                gx: zeros(3 * HID),
                gh: zeros(3 * HID),
                coefs: zeros(DF * 2 * ORDER),
                p10: zeros(10 * DF),
                q10: zeros(10 * DF),
                mask: [0.0; ERB],
                out_re: zeros(FREQS),
                out_im: zeros(FREQS),
                frame: zeros(FFT),
            },
        };
        engine.set_level(level);
        engine
    }

    pub fn set_level(&mut self, level: Level) {
        self.limit = if level == Level::Soft { Some(10f32.powf(-12.0 / 20.0)) } else { None };
        self.post = if level == Level::Max { Some(0.02) } else { None };
    }

    pub fn tune(&mut self, limit_db: f32, between: bool) {
        self.limit = if limit_db >= 100.0 { None } else { Some(10f32.powf(-limit_db.max(0.0) / 20.0)) };
        self.post = between.then_some(0.02);
    }

    pub fn set_thresholds(&mut self, min: f32, erb: f32, df: f32) {
        self.thresholds = (min, erb, df);
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        self.pending.extend_from_slice(input);
        let frames = self.pending.len() / HOP;
        let mut out = vec![0.0; frames * HOP];
        let mut frame = [0.0f32; HOP];
        for (n, chunk) in out.chunks_exact_mut(HOP).enumerate() {
            frame.copy_from_slice(&self.pending[n * HOP..(n + 1) * HOP]);
            self.frame(&frame, chunk);
        }
        self.pending.drain(..frames * HOP);
        out
    }

    pub fn frame(&mut self, input: &[f32], output: &mut [f32]) -> f32 {
        let power = input.iter().map(|x| x * x).sum::<f32>() / HOP as f32;
        self.quiet = if power < 1e-7 { self.quiet + 1 } else { 0 };
        if self.quiet > 5 {
            output.fill(0.0);
            return -15.0;
        }
        self.analyze(input);
        let lsnr = self.encode();
        let (min, erb_max, df_max) = self.thresholds;
        let (gains, zeros, deep) = if lsnr < min {
            (false, true, false)
        } else if lsnr > erb_max {
            (false, false, false)
        } else if lsnr > df_max {
            (true, false, false)
        } else {
            (true, false, true)
        };
        if gains {
            self.decode_mask();
        }
        if deep {
            self.decode_df();
        }
        self.apply(gains, zeros, deep);
        self.synthesize(output);
        lsnr
    }

    fn analyze(&mut self, input: &[f32]) {
        let s = &mut self.s;
        for i in 0..HOP {
            s.frame[i] = self.analysis[i] * self.window[i];
            s.frame[HOP + i] = input[i] * self.window[HOP + i];
        }
        self.analysis.copy_from_slice(input);
        self.ring_re.rotate_left(1);
        self.ring_im.rotate_left(1);
        let (re, im) = (&mut self.ring_re[ORDER - 1], &mut self.ring_im[ORDER - 1]);
        self.fft.forward(&s.frame, re, im, 1.0 / FFT as f32);
    }

    fn encode(&mut self) -> f32 {
        let net = Arc::clone(&self.net);
        let (re, im) = (&self.ring_re[ORDER - 1], &self.ring_im[ORDER - 1]);
        self.erb_hist.rotate_left(1);
        self.spec_hist.rotate_left(1);
        let mut at = 0;
        for (b, &size) in self.bands.iter().enumerate() {
            let mut power = 0.0;
            for k in at..at + size {
                power += re[k] * re[k] + im[k] * im[k];
            }
            at += size;
            let db = 10.0 * (power / size as f32 + 1e-10).log10();
            self.mean_norm[b] = db * (1.0 - ALPHA) + self.mean_norm[b] * ALPHA;
            self.erb_hist[2][b] = (db - self.mean_norm[b]) / 40.0;
        }
        for k in 0..DF {
            let mag = (re[k] * re[k] + im[k] * im[k]).sqrt();
            self.unit_norm[k] = mag * (1.0 - ALPHA) + self.unit_norm[k] * ALPHA;
            let d = self.unit_norm[k].sqrt();
            self.spec_hist[2][k] = re[k] / d;
            self.spec_hist[2][DF + k] = im[k] / d;
        }
        let s = &mut self.s;
        padded(&[&self.erb_hist[0], &self.erb_hist[1], &self.erb_hist[2]], &mut s.b);
        conv3x3(&net.erb0, &s.b, 1, ERB, &mut s.e0, Some(&net.erb0_b));
        relu(&mut s.e0);
        depthwise(&net.erb_dw[0], &s.e0, &mut s.a[..CH * 16], 32, 2);
        kernel::pointwise(&net.erb_pw[0], &net.erb_pb[0], &s.a, &mut s.e1, CH, 16);
        relu(&mut s.e1);
        depthwise(&net.erb_dw[1], &s.e1, &mut s.a[..CH * 8], 16, 2);
        kernel::pointwise(&net.erb_pw[1], &net.erb_pb[1], &s.a, &mut s.e2, CH, 8);
        relu(&mut s.e2);
        depthwise(&net.erb_dw[2], &s.e2, &mut s.a[..CH * 8], 8, 1);
        kernel::pointwise(&net.erb_pw[2], &net.erb_pb[2], &s.a, &mut s.e3, CH, 8);
        relu(&mut s.e3);
        let h = &self.spec_hist;
        padded(&[&h[0][..DF], &h[1][..DF], &h[2][..DF], &h[0][DF..], &h[1][DF..], &h[2][DF..]], &mut s.b);
        conv3x3(&net.df0, &s.b, 2, DF, &mut s.a, None);
        kernel::pointwise(&net.df0_pw, &net.df0_pb, &s.a, &mut s.c0, CH, DF);
        relu(&mut s.c0);
        depthwise(&net.df1_dw, &s.c0, &mut s.a[..CH * 48], DF, 2);
        kernel::pointwise(&net.df1_pw, &net.df1_pb, &s.a, &mut s.c1, CH, 48);
        relu(&mut s.c1);
        flatten(&s.c1, 48, &mut s.flat);
        kernel::grouped(&net.fc_emb, &s.flat[..CH * 48], &mut s.emb, 32);
        relu(&mut s.emb);
        flatten(&s.e3, 8, &mut s.flat[..512]);
        for (e, v) in s.emb.iter_mut().zip(&s.flat[..512]) {
            *e += v;
        }
        kernel::grouped(&net.enc_in, &s.emb, &mut s.v256, 16);
        relu(&mut s.v256);
        gru(&net.enc_gru, &s.v256, &mut self.h_enc, &mut s.gx, &mut s.gh);
        kernel::grouped(&net.enc_out, &self.h_enc, &mut s.emb, 16);
        relu(&mut s.emb);
        let z: f32 = s.emb.iter().zip(&net.lsnr_w).map(|(a, b)| a * b).sum::<f32>() + net.lsnr_b;
        sigmoid(z) * 50.0 - 15.0
    }

    fn decode_mask(&mut self) {
        let net = Arc::clone(&self.net);
        let s = &mut self.s;
        kernel::grouped(&net.erb_in, &s.emb, &mut s.v256, 16);
        relu(&mut s.v256);
        let [first, second] = &mut self.h_erb;
        gru(&net.erb_gru[0], &s.v256, first, &mut s.gx, &mut s.gh);
        gru(&net.erb_gru[1], first, second, &mut s.gx, &mut s.gh);
        kernel::grouped(&net.erb_out, second, &mut s.flat[..512], 16);
        relu(&mut s.flat[..512]);
        let x = &mut s.d;
        for c in 0..CH {
            for f in 0..8 {
                let v = net.skip_w[0][c] * s.e3[c * 8 + f] + net.skip_b[0][c];
                x[c * 8 + f] = v.max(0.0) + s.flat[f * CH + c];
            }
        }
        depthwise(&net.up_dw[0], &x[..CH * 8], &mut s.a[..CH * 8], 8, 1);
        kernel::pointwise(&net.up_pw[0], &net.up_pb[0], &s.a, &mut s.b[..CH * 8], CH, 8);
        relu(&mut s.b[..CH * 8]);
        let stages: [(&[f32], usize, usize); 2] = [(&s.e2, 8, 1), (&s.e1, 16, 2)];
        let mut width = 8;
        for (stage, (skip, size, w_at)) in stages.iter().enumerate() {
            debug_assert_eq!(width, *size);
            for c in 0..CH {
                for f in 0..width {
                    let v = net.skip_w[stage + 1][c] * skip[c * width + f] + net.skip_b[stage + 1][c];
                    x[c * width + f] = v.max(0.0) + s.b[c * width + f];
                }
            }
            transposed(&net.up_dw[*w_at], &x[..CH * width], &mut s.a, width);
            width *= 2;
            kernel::pointwise(&net.up_pw[*w_at], &net.up_pb[*w_at], &s.a, &mut s.b[..CH * width], CH, width);
            relu(&mut s.b[..CH * width]);
        }
        let span = ERB + 2;
        for c in 0..CH {
            let row = &mut s.a[c * span..(c + 1) * span];
            row[0] = 0.0;
            row[span - 1] = 0.0;
            for f in 0..ERB {
                let v = net.skip_w[3][c] * s.e0[c * ERB + f] + net.skip_b[3][c];
                row[f + 1] = v.max(0.0) + s.b[c * ERB + f];
            }
        }
        let mut acc = [net.mask_b; ERB];
        for c in 0..CH {
            for t in 0..3 {
                kernel::axpy(&mut acc, net.mask_w[c * 3 + t], &s.a[c * span + t..c * span + t + ERB]);
            }
        }
        kernel::sigmoids(&mut acc);
        s.mask = acc;
    }

    fn decode_df(&mut self) {
        let net = Arc::clone(&self.net);
        let s = &mut self.s;
        kernel::grouped(&net.df_in, &s.emb, &mut s.v256, 8);
        relu(&mut s.v256);
        let [first, second] = &mut self.h_df;
        gru(&net.df_gru[0], &s.v256, first, &mut s.gx, &mut s.gh);
        gru(&net.df_gru[1], first, second, &mut s.gx, &mut s.gh);
        kernel::grouped(&net.df_skip, &s.emb, &mut s.w256, 16);
        for (a, b) in s.w256.iter_mut().zip(second.iter()) {
            *a += b;
        }
        kernel::grouped(&net.df_out, &s.w256, &mut s.coefs, 16);
        kernel::tanhs(&mut s.coefs);
        for g in 0..2 {
            let hist = &mut self.convp_hist[g];
            hist.copy_within(32 * DF.., 0);
            hist[4 * 32 * DF..].copy_from_slice(&s.c0[g * 32 * DF..(g + 1) * 32 * DF]);
            kernel::pointwise(&net.convp[g * 5 * 160..(g + 1) * 5 * 160], &[], hist, &mut s.p10[g * 5 * DF..(g + 1) * 5 * DF], 160, DF);
        }
        kernel::pointwise(&net.convp_pw, &net.convp_pb, &s.p10, &mut s.q10, 10, DF);
        relu(&mut s.q10);
        for f in 0..DF {
            for o in 0..10 {
                s.coefs[f * 10 + o] += s.q10[o * DF + f];
            }
        }
    }

    fn apply(&mut self, gains: bool, zeros: bool, deep: bool) {
        let s = &mut self.s;
        let (nr, ni) = (&self.ring_re[ORDER - 1 - LOOK], &self.ring_im[ORDER - 1 - LOOK]);
        s.out_re.copy_from_slice(nr);
        s.out_im.copy_from_slice(ni);
        if gains || zeros {
            let mut at = 0;
            for (b, &size) in self.bands.iter().enumerate() {
                let g = if gains { s.mask[b] } else { 0.0 };
                for k in at..at + size {
                    s.out_re[k] *= g;
                    s.out_im[k] *= g;
                }
                at += size;
            }
        }
        if deep {
            for f in 0..DF {
                let (mut ar, mut ai) = (0.0, 0.0);
                for o in 0..ORDER {
                    let (xr, xi) = (self.ring_re[o][f], self.ring_im[o][f]);
                    let (cr, ci) = (s.coefs[f * 10 + o * 2], s.coefs[f * 10 + o * 2 + 1]);
                    ar += xr * cr - xi * ci;
                    ai += xr * ci + xi * cr;
                }
                s.out_re[f] = ar;
                s.out_im[f] = ai;
            }
        }
        if let (true, Some(beta)) = (gains, self.post) {
            for k in 0..FREQS / 4 * 4 {
                let noisy = (nr[k] * nr[k] + ni[k] * ni[k]).sqrt();
                let clean = (s.out_re[k] * s.out_re[k] + s.out_im[k] * s.out_im[k]).sqrt();
                let g = (clean / (noisy + 1e-12)).clamp(1e-12, 1.0);
                let g_sin = g * (g * PI / 2.0).sin();
                let pf = ((beta + 1.0) * g / (1.0 + beta * (g / g_sin).powi(2))) / g;
                s.out_re[k] *= pf;
                s.out_im[k] *= pf;
            }
        }
        if let Some(lim) = self.limit {
            for k in 0..FREQS {
                s.out_re[k] = s.out_re[k] * (1.0 - lim) + nr[k] * lim;
                s.out_im[k] = s.out_im[k] * (1.0 - lim) + ni[k] * lim;
            }
        }
    }

    fn synthesize(&mut self, output: &mut [f32]) {
        let s = &mut self.s;
        self.fft.inverse(&s.out_re, &s.out_im, &mut s.frame);
        for i in 0..FFT {
            s.frame[i] *= self.window[i];
        }
        for i in 0..HOP {
            output[i] = s.frame[i] + self.synthesis[i];
        }
        self.synthesis.copy_from_slice(&s.frame[HOP..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fft_matches_a_plain_dft() {
        let mut fft = Fft::new(FFT);
        let mut seed = 3u32;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed as f32 / u32::MAX as f32 - 0.5
        };
        let input: Vec<f32> = (0..FFT).map(|_| next()).collect();
        let (mut re, mut im) = (vec![0.0; FREQS], vec![0.0; FREQS]);
        fft.forward(&input, &mut re, &mut im, 1.0);
        let mut back = vec![0.0; FFT];
        fft.inverse(&re, &im, &mut back);
        for (n, (a, b)) in input.iter().zip(&back).enumerate() {
            assert!((a * FFT as f32 - b).abs() < 1e-3, "{n}: {a} {b}");
        }
        for k in 0..FREQS {
            let (mut dr, mut di) = (0.0f64, 0.0f64);
            for (n, x) in input.iter().enumerate() {
                let t = -2.0 * std::f64::consts::PI * (k * n) as f64 / FFT as f64;
                dr += *x as f64 * t.cos();
                di += *x as f64 * t.sin();
            }
            assert!((re[k] as f64 - dr).abs() < 1e-3 && (im[k] as f64 - di).abs() < 1e-3, "{k}: {} {} contra {dr} {di}", re[k], im[k]);
        }
    }

    #[test]
    fn the_bands_cover_every_bin() {
        let b = bands();
        assert_eq!(b.iter().sum::<usize>(), FREQS);
        assert_eq!(&b[..6], &[2, 2, 2, 2, 2, 2]);
    }

    #[test]
    fn the_weights_load_and_half_floats_read_right() {
        assert_eq!(half(0x3c00), 1.0);
        assert_eq!(half(0xc000), -2.0);
        assert!((half(0x3555) - 0.33325).abs() < 1e-4);
        let net = shared(false);
        let size = match &net.enc_gru.w {
            Mat::Byte(w, _) => w.len(),
            Mat::Half(w) => w.len(),
            Mat::Full(w) => w.len(),
        };
        assert_eq!(size, 3 * HID * HID);
    }

    #[test]
    fn silence_stays_silent_and_the_length_is_kept() {
        let mut engine = Profundo::new(Level::Normal);
        let out = engine.process(&vec![0.0; HOP * 20 + 100]);
        assert_eq!(out.len(), HOP * 20);
        assert!(out.iter().all(|v| v.abs() < 1e-6));
    }

    #[test]
    fn a_muted_mic_costs_nothing_and_says_nothing() {
        let mut engine = Profundo::new(Level::Normal);
        let (input, mut output) = ([0.0f32; HOP], [1.0f32; HOP]);
        for _ in 0..6 {
            engine.frame(&input, &mut output);
        }
        let started = std::time::Instant::now();
        for _ in 0..1000 {
            assert_eq!(engine.frame(&input, &mut output), -15.0);
        }
        assert!(started.elapsed().as_millis() < 20, "{:?}", started.elapsed());
        assert!(output.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn a_passthrough_rebuilds_the_signal() {
        let mut engine = Profundo::new(Level::Normal);
        engine.set_thresholds(-100.0, -100.0, -100.0);
        let input: Vec<f32> = (0..HOP * 30).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        let out = engine.process(&input);
        for i in HOP * 4..HOP * 20 {
            assert!((out[i + DELAY] - input[i]).abs() < 1e-4, "{i}: {} {}", out[i + DELAY], input[i]);
        }
    }
}

#[cfg(test)]
mod medidas {
    use super::*;

    fn time(name: &str, times: usize, mut f: impl FnMut()) {
        for _ in 0..10 {
            f();
        }
        let started = std::time::Instant::now();
        for _ in 0..times {
            f();
        }
        println!("[medida] {name:28} {:8.2} µs", started.elapsed().as_secs_f64() * 1e6 / times as f64);
    }

    #[test]
    #[ignore]
    fn kernels() {
        let net = shared(false);
        let x: Vec<f32> = (0..512).map(|i| (i as f32 * 0.37).sin()).collect();
        let mut out = vec![0.0f32; 768];
        let mut gx = vec![0.0f32; 768];
        let mut gh = vec![0.0f32; 768];
        let mut h = vec![0.1f32; 256];
        time("producto de una gru", 2000, || product(&net.enc_gru.w, &x[..256], &net.enc_gru.b[..768], &mut out));
        time("gru entera", 2000, || gru(&net.enc_gru, &x[..256], &mut h, &mut gx, &mut gh));
        let mut t = vec![0.0f32; 768];
        time("768 tanh", 2000, || t.iter_mut().zip(&out).for_each(|(a, b)| *a = b.tanh()));
        time("768 sigmoides", 2000, || t.iter_mut().zip(&out).for_each(|(a, b)| *a = sigmoid(*b)));
        let xs: Vec<f32> = (0..CH * DF).map(|i| (i as f32 * 0.01).cos()).collect();
        let mut o = vec![0.0f32; CH * DF];
        time("puntual 64x64x96", 2000, || kernel::pointwise(&net.df0_pw, &net.df0_pb, &xs, &mut o, CH, DF));
        let mut e = vec![0.0f32; 512];
        time("lineal agrupada fc_emb", 2000, || kernel::grouped(&net.fc_emb, &xs[..CH * 48], &mut e, 32));
        let mut pad = vec![0.0f32; 6 * 98];
        let rows: Vec<&[f32]> = (0..6).map(|r| &xs[r * 96..(r + 1) * 96]).collect();
        time("conv 3x3 de df", 2000, || {
            padded(&rows, &mut pad);
            conv3x3(&net.df0, &pad, 2, DF, &mut o, None)
        });
        let mut d = vec![0.0f32; CH * 48];
        time("depthwise 96 a 48", 2000, || depthwise(&net.df1_dw, &xs, &mut d, DF, 2));
        let mut engine = Profundo::new(Level::Normal);
        let input: Vec<f32> = (0..HOP).map(|i| (i as f32 * 0.07).sin() * 0.1).collect();
        let mut output = vec![0.0f32; HOP];
        time("cuadro completo", 500, || {
            engine.frame(&input, &mut output);
        });
    }
}
