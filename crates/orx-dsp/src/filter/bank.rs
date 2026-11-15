//! A bank of state variable filters run side by side.
//!
//! Filter banks (graphic EQs, vocoders, sub-band pitch shifting, the decay
//! filters of an FDN) run many independent recursive filters on the same
//! samples. A recursive filter cannot be vectorised along time, but N of
//! them can be along the bank: the state and coefficients are stored as
//! structure-of-arrays in lanes of [`LANES`], and each sample updates
//! every lane with the same instructions. This is the SIMD pattern of the
//! suite (see `orx-simd`); the kernel is dispatched at run time.

use super::Shape;
use super::svf::SvfCoeffs;
use crate::alloc;

/// Filters per vector group (AVX2 width; NEON does it in two halves)
pub const LANES: usize = 8;

type Lane = [f32; LANES];

/// N band-pass SVFs fed by one input, their outputs weighted and summed
pub struct SvfBank {
    bands: usize,
    a1: Box<[Lane]>,
    a2: Box<[Lane]>,
    a3: Box<[Lane]>,
    /// Output weight of the band output (`k` × band gain: 0 dB peak × gain)
    w: Box<[Lane]>,
    ic1: Box<[Lane]>,
    ic2: Box<[Lane]>,
}

impl SvfBank {
    /// A bank of `bands` filters, all passing nothing until set
    pub fn new(bands: usize) -> Self {
        let groups = bands.div_ceil(LANES).max(1);
        Self {
            bands,
            a1: alloc::filled(groups),
            a2: alloc::filled(groups),
            a3: alloc::filled(groups),
            w: alloc::filled(groups),
            ic1: alloc::filled(groups),
            ic2: alloc::filled(groups),
        }
    }

    pub fn len(&self) -> usize {
        self.bands
    }

    pub fn is_empty(&self) -> bool {
        self.bands == 0
    }

    /// Make band `i` a 0 dB band pass at `freq` with `q`, weighted by `gain`
    pub fn set_band(&mut self, i: usize, sample_rate: f64, freq: f64, q: f64, gain: f32) {
        if i >= self.bands {
            return;
        }
        let c = SvfCoeffs::new(Shape::Bandpass, sample_rate, freq, q, 0.0);
        let (g, l) = (i / LANES, i % LANES);
        if let (Some(a1), Some(a2), Some(a3), Some(w)) =
            (self.a1.get_mut(g), self.a2.get_mut(g), self.a3.get_mut(g), self.w.get_mut(g))
        {
            a1[l] = c.a1;
            a2[l] = c.a2;
            a3[l] = c.a3;
            w[l] = c.k * gain;
        }
    }

    pub fn reset(&mut self) {
        self.ic1.fill([0.0; LANES]);
        self.ic2.fill([0.0; LANES]);
    }

    /// `output[n]` = Σ weight_i · bandpass_i(`input[n]`). Real-time safe.
    pub fn process(&mut self, input: &[f32], output: &mut [f32]) {
        bank_kernel(input, output, &self.a1, &self.a2, &self.a3, &self.w, &mut self.ic1, &mut self.ic2);
    }

    /// Same as [`SvfBank::process`] with a given instruction set (tests,
    /// benches)
    pub fn process_with(&mut self, isa: orx_simd::Isa, input: &[f32], output: &mut [f32]) {
        bank_kernel_with(isa, input, output, &self.a1, &self.a2, &self.a3, &self.w, &mut self.ic1, &mut self.ic2);
    }
}

orx_simd::kernel! {
    /// The bank, one sample at a time, all lanes at once
    #[allow(clippy::too_many_arguments)]
    fn bank_kernel, bank_kernel_with(
        input: &[f32],
        output: &mut [f32],
        a1: &[Lane],
        a2: &[Lane],
        a3: &[Lane],
        w: &[Lane],
        ic1: &mut [Lane],
        ic2: &mut [Lane],
    ) {
        for (x, y) in input.iter().zip(output.iter_mut()) {
            let v0 = *x;
            let mut acc: Lane = [0.0; LANES];
            for (((((a1, a2), a3), w), s1), s2) in a1
                .iter()
                .zip(a2)
                .zip(a3)
                .zip(w)
                .zip(ic1.iter_mut())
                .zip(ic2.iter_mut())
            {
                for l in 0..LANES {
                    let v3 = v0 - s2[l];
                    let v1 = a1[l] * s1[l] + a2[l] * v3;
                    let v2 = s2[l] + a2[l] * s1[l] + a3[l] * v3;
                    s1[l] = 2.0 * v1 - s1[l];
                    s2[l] = 2.0 * v2 - s2[l];
                    acc[l] += w[l] * v1;
                }
            }
            // Pairwise sum: same order on every path
            let mut sum = 0.0;
            for pair in acc.as_chunks::<2>().0 {
                sum += pair[0] + pair[1];
            }
            *y = sum;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::svf::Svf;
    use super::*;
    use orx_simd::Isa;

    const FS: f64 = 48_000.0;

    fn bank(n: usize) -> SvfBank {
        let mut b = SvfBank::new(n);
        for i in 0..n {
            let f = 60.0 * 2f64.powf(i as f64 / 3.0);
            b.set_band(i, FS, f, 4.3, 1.0 / (1.0 + i as f32));
        }
        b
    }

    fn signal() -> Vec<f32> {
        (0..4096).map(|n| ((n * 7919 % 1013) as f32 / 506.5 - 1.0) * 0.5).collect()
    }

    #[test]
    fn equals_separate_filters() {
        let n = 21;
        let mut b = bank(n);
        let input = signal();
        let mut out = vec![0.0; input.len()];
        b.process(&input, &mut out);
        let mut filters: Vec<(Svf, f32)> = (0..n)
            .map(|i| {
                let f = 60.0 * 2f64.powf(i as f64 / 3.0);
                (Svf::new(SvfCoeffs::new(Shape::Bandpass, FS, f, 4.3, 0.0)), 1.0 / (1.0 + i as f32))
            })
            .collect();
        for (x, y) in input.iter().zip(&out) {
            let expected: f32 = filters.iter_mut().map(|(f, g)| f.process(*x) * *g).sum();
            assert!((expected - y).abs() < 1e-4, "{expected} vs {y}");
        }
    }

    #[test]
    fn isa_paths_agree() {
        let input = signal();
        let mut reference = vec![0.0; input.len()];
        bank(40).process_with(Isa::Baseline, &input, &mut reference);
        let mut fast = vec![0.0; input.len()];
        bank(40).process_with(Isa::Avx2Fma, &input, &mut fast);
        let worst = reference.iter().zip(&fast).map(|(a, b)| (a - b).abs()).fold(0.0, f32::max);
        assert!(worst < 1e-5, "{worst}");
    }
}
