//! Numerical primitives: the normal distribution, robust summaries, small
//! dense linear algebra and a deterministic random number generator.

use std::f64::consts::PI;

const INV_SQRT_2PI: f64 = 0.398_942_280_401_432_7;

/// Standard normal density.
pub fn norm_pdf(x: f64) -> f64 {
    INV_SQRT_2PI * (-0.5 * x * x).exp()
}

/// Standard normal distribution function, accurate to about 1e-15.
///
/// Uses the complementary error function with a rational Chebyshev
/// approximation (W. J. Cody, 1969) so the tails keep relative accuracy.
pub fn norm_cdf(x: f64) -> f64 {
    0.5 * erfc(-x / std::f64::consts::SQRT_2)
}

fn erfc(x: f64) -> f64 {
    let z = x.abs();
    let r = if z < 0.5 {
        const P: [f64; 5] = [
            3.209_377_589_138_469_4e3,
            3.774_852_376_853_020_2e2,
            1.138_641_541_510_501_6e2,
            3.161_123_743_870_565_6,
            1.857_777_061_846_031_5e-1,
        ];
        const Q: [f64; 4] = [
            2.844_236_833_439_170_6e3,
            1.282_616_526_077_372_3e3,
            2.440_246_379_344_441_7e2,
            2.360_129_095_234_412_1e1,
        ];
        let y = z * z;
        let num = (((P[4] * y + P[3]) * y + P[2]) * y + P[1]) * y + P[0];
        let den = (((y + Q[3]) * y + Q[2]) * y + Q[1]) * y + Q[0];
        1.0 - z * num / den
    } else if z < 4.0 {
        const P: [f64; 9] = [
            1.230_339_354_797_997_2e3,
            2.051_078_377_826_071_5e3,
            1.712_047_612_634_070_6e3,
            8.819_522_212_417_691e2,
            2.986_351_381_974_001_3e2,
            6.611_919_063_714_163e1,
            8.883_149_794_388_376,
            5.641_884_969_886_701e-1,
            2.153_115_354_744_038_5e-8,
        ];
        const Q: [f64; 8] = [
            1.230_339_354_803_749_4e3,
            3.439_367_674_143_721_6e3,
            4.362_619_090_143_247e3,
            3.290_799_235_733_459_6e3,
            1.621_389_574_566_690_2e3,
            5.371_811_018_620_098_6e2,
            1.176_939_508_913_125e2,
            1.574_492_611_070_983_5e1,
        ];
        let mut num = P[8];
        for k in (0..8).rev() {
            num = num * z + P[k];
        }
        let mut den = 1.0;
        for k in (0..8).rev() {
            den = den * z + Q[k];
        }
        (-z * z).exp() * num / den
    } else {
        const P: [f64; 6] = [
            -6.587_491_615_298_378e-4,
            -1.608_378_514_874_227_7e-2,
            -1.257_817_261_112_292_5e-1,
            -3.603_448_999_498_044_4e-1,
            -3.053_266_349_612_323_4e-1,
            -1.631_538_713_730_209_8e-2,
        ];
        const Q: [f64; 5] = [
            2.335_204_976_268_691_8e-3,
            6.051_834_131_244_131_9e-2,
            5.279_051_029_514_284_1e-1,
            1.872_952_849_923_460_4,
            2.568_520_192_289_822_4,
        ];
        let y = 1.0 / (z * z);
        let num = ((((P[5] * y + P[4]) * y + P[3]) * y + P[2]) * y + P[1]) * y + P[0];
        let den = ((((y + Q[4]) * y + Q[3]) * y + Q[2]) * y + Q[1]) * y + Q[0];
        (-z * z).exp() / z * (1.0 / PI.sqrt() + y * num / den)
    };
    if x < 0.0 {
        2.0 - r
    } else {
        r
    }
}

/// Inverse of the standard normal distribution function.
///
/// Peter Acklam's rational approximation followed by one step of Halley's
/// method, which brings the result to full double precision.
pub fn norm_inv(p: f64) -> f64 {
    if p <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if p >= 1.0 {
        return f64::INFINITY;
    }
    const A: [f64; 6] = [
        -3.969_683_028_665_376e1,
        2.209_460_984_245_205e2,
        -2.759_285_104_469_687e2,
        1.383_577_518_672_69e2,
        -3.066_479_806_614_716e1,
        2.506_628_277_459_239,
    ];
    const B: [f64; 5] = [
        -5.447_609_879_822_406e1,
        1.615_858_368_580_409e2,
        -1.556_989_798_598_866e2,
        6.680_131_188_771_972e1,
        -1.328_068_155_288_572e1,
    ];
    const C: [f64; 6] = [
        -7.784_894_002_430_293e-3,
        -3.223_964_580_411_365e-1,
        -2.400_758_277_161_838,
        -2.549_732_539_343_734,
        4.374_664_141_464_968,
        2.938_163_982_698_783,
    ];
    const D: [f64; 4] = [
        7.784_695_709_041_462e-3,
        3.224_671_290_700_398e-1,
        2.445_134_137_142_996,
        3.754_408_661_907_416,
    ];
    let lo = 0.02425;
    let x = if p < lo {
        let q = (-2.0 * p.ln()).sqrt();
        (((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    } else if p <= 1.0 - lo {
        let q = p - 0.5;
        let r = q * q;
        (((((A[0] * r + A[1]) * r + A[2]) * r + A[3]) * r + A[4]) * r + A[5]) * q
            / (((((B[0] * r + B[1]) * r + B[2]) * r + B[3]) * r + B[4]) * r + 1.0)
    } else {
        let q = (-2.0 * (1.0 - p).ln()).sqrt();
        -(((((C[0] * q + C[1]) * q + C[2]) * q + C[3]) * q + C[4]) * q + C[5])
            / ((((D[0] * q + D[1]) * q + D[2]) * q + D[3]) * q + 1.0)
    };
    let e = norm_cdf(x) - p;
    let u = e / norm_pdf(x);
    x - u / (1.0 + 0.5 * x * u)
}

/// Linear-interpolated quantile of a slice. Sorts the slice in place.
pub fn quantile(xs: &mut [f64], p: f64) -> f64 {
    if xs.is_empty() {
        return f64::NAN;
    }
    xs.sort_by(|a, b| a.total_cmp(b));
    quantile_sorted(xs, p)
}

/// Linear-interpolated quantile of an already sorted slice.
pub fn quantile_sorted(xs: &[f64], p: f64) -> f64 {
    if xs.is_empty() {
        return f64::NAN;
    }
    let h = p.clamp(0.0, 1.0) * (xs.len() - 1) as f64;
    let lo = h.floor() as usize;
    let hi = (lo + 1).min(xs.len() - 1);
    xs[lo] + (h - lo as f64) * (xs[hi] - xs[lo])
}

pub fn median(xs: &mut [f64]) -> f64 {
    quantile(xs, 0.5)
}

pub fn mean(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return f64::NAN;
    }
    xs.iter().sum::<f64>() / xs.len() as f64
}

/// Median absolute deviation scaled to estimate a normal standard deviation.
pub fn mad_sigma(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return f64::NAN;
    }
    let mut v = xs.to_vec();
    let m = median(&mut v);
    for x in v.iter_mut() {
        *x = (*x - m).abs();
    }
    1.482_602_218_505_602 * median(&mut v)
}

/// Variance after clamping to the `p` and `1 - p` quantiles. A compromise
/// between the MAD, which ignores the tails that matter for credit, and the
/// raw variance, which a handful of defaults would dominate.
pub fn winsor_var(xs: &[f64], p: f64) -> f64 {
    if xs.len() < 2 {
        return f64::NAN;
    }
    let mut v = xs.to_vec();
    v.sort_by(|a, b| a.total_cmp(b));
    let lo = quantile_sorted(&v, p);
    let hi = quantile_sorted(&v, 1.0 - p);
    let n = v.len() as f64;
    let mut s = 0.0;
    let mut ss = 0.0;
    for x in &v {
        let c = x.clamp(lo, hi);
        s += c;
        ss += c * c;
    }
    (ss - s * s / n) / (n - 1.0)
}

/// Solves the symmetric positive definite system `A x = b` in place by
/// Cholesky factorisation. `a` is row-major `n` by `n`. Returns false if the
/// matrix is not positive definite.
pub fn solve_spd(a: &mut [f64], b: &mut [f64], n: usize) -> bool {
    for j in 0..n {
        let mut d = a[j * n + j];
        for k in 0..j {
            d -= a[j * n + k] * a[j * n + k];
        }
        if d <= 0.0 || !d.is_finite() {
            return false;
        }
        let d = d.sqrt();
        a[j * n + j] = d;
        for i in (j + 1)..n {
            let mut s = a[i * n + j];
            for k in 0..j {
                s -= a[i * n + k] * a[j * n + k];
            }
            a[i * n + j] = s / d;
        }
    }
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i {
            s -= a[i * n + k] * b[k];
        }
        b[i] = s / a[i * n + i];
    }
    for i in (0..n).rev() {
        let mut s = b[i];
        for k in (i + 1)..n {
            s -= a[k * n + i] * b[k];
        }
        b[i] = s / a[i * n + i];
    }
    true
}

/// Accumulates the normal equations of a least squares problem one row at a
/// time, so a regression over millions of observations needs only `k` squared
/// numbers of state.
pub struct Ols {
    k: usize,
    xtx: Vec<f64>,
    xty: Vec<f64>,
    pub n: f64,
    pub yy: f64,
}

impl Ols {
    pub fn new(k: usize) -> Self {
        Ols { k, xtx: vec![0.0; k * k], xty: vec![0.0; k], n: 0.0, yy: 0.0 }
    }

    pub fn add(&mut self, x: &[f64], y: f64, w: f64) {
        for i in 0..self.k {
            let wi = w * x[i];
            self.xty[i] += wi * y;
            for j in 0..=i {
                self.xtx[i * self.k + j] += wi * x[j];
            }
        }
        self.n += w;
        self.yy += w * y * y;
    }

    /// The same regression with another's observations taken out.
    pub fn without(&self, other: &Ols) -> Ols {
        let mut o = Ols { k: self.k, xtx: self.xtx.clone(), xty: self.xty.clone(), n: self.n - other.n, yy: self.yy - other.yy };
        for (a, b) in o.xtx.iter_mut().zip(&other.xtx) {
            *a -= b;
        }
        for (a, b) in o.xty.iter_mut().zip(&other.xty) {
            *a -= b;
        }
        o
    }

    /// Returns the coefficients and the weighted residual sum of squares. A
    /// small ridge keeps a rank-deficient design solvable; the coefficients on
    /// its redundant columns then shrink to zero, which is the wanted answer.
    pub fn solve(&self) -> Option<(Vec<f64>, f64)> {
        let k = self.k;
        let mut a = vec![0.0; k * k];
        for i in 0..k {
            for j in 0..=i {
                a[i * k + j] = self.xtx[i * k + j];
                a[j * k + i] = self.xtx[i * k + j];
            }
            a[i * k + i] += 1e-12 * (1.0 + self.xtx[i * k + i]);
        }
        let mut b = self.xty.clone();
        if !solve_spd(&mut a, &mut b, k) {
            return None;
        }
        let mut sse = self.yy;
        for i in 0..k {
            sse -= b[i] * self.xty[i];
        }
        Some((b, sse.max(0.0)))
    }
}

/// xoshiro256** seeded through splitmix64. Deterministic across platforms,
/// which the standard library's generator does not promise.
#[derive(Clone)]
pub struct Rng {
    s: [u64; 4],
    spare: Option<f64>,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut z = seed;
        let mut next = || {
            z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut x = z;
            x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            x ^ (x >> 31)
        };
        Rng { s: [next(), next(), next(), next()], spare: None }
    }

    pub fn next_u64(&mut self) -> u64 {
        let r = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        r
    }

    /// Uniform on the open interval (0, 1).
    pub fn uniform(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
    }

    /// Standard normal by the polar Box-Muller method.
    pub fn normal(&mut self) -> f64 {
        if let Some(z) = self.spare.take() {
            return z;
        }
        loop {
            let u = 2.0 * self.uniform() - 1.0;
            let v = 2.0 * self.uniform() - 1.0;
            let s = u * u + v * v;
            if s > 0.0 && s < 1.0 {
                let m = (-2.0 * s.ln() / s).sqrt();
                self.spare = Some(v * m);
                return u * m;
            }
        }
    }

    /// Exponential with unit rate.
    pub fn exponential(&mut self) -> f64 {
        -self.uniform().ln()
    }
}
