// CEP:FILE: crates/mlkp-autotune/src/gp.rs
// CEP:WHAT: The Phase 2 Gaussian-process surrogate: RBF-kernel GP regression over the candidate feature space with a packed Cholesky solve, plus the Expected Improvement acquisition; std-only and deterministic.
// CEP:WHY: Design Phase 2 (Bayesian optimization) needs per-point posterior uncertainty to balance exploration and exploitation; the ridge cost model only exposes a global residual spread, so the acquisition cannot distinguish an uncertain region from a certain one. The GP posterior gives every candidate its own variance, and EI turns mean and variance into one acquisition value.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: fit returns None below the minimum point count or when the kernel stays non-positive-definite after the jitter retries (never panics); predict clamps variance at zero and returns finite values for finite inputs.
// CEP:ASSUMES: features are finite; the fit set is capped so the O(n^3) solve stays inside the compile budget; targets are latencies in nanoseconds (scale-free through centering and normalization).
// CEP:COST: fit O(n^3) over at most GP_MAX_FIT_POINTS observations (n <= 160 keeps the solve sub-millisecond in release); predict O(n) per candidate.
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs and benches/artifacts/compiler.md.

//! Gaussian-process surrogate and Expected Improvement acquisition for Phase 2.

#![deny(missing_docs)]

use mlkp_config::limits::{
    GP_LENGTHSCALE_FLOOR, GP_MAX_FIT_POINTS, GP_MIN_FIT_POINTS, GP_NOISE_REL,
};

/// A fitted GP regression model over the six-dimensional candidate features.
pub struct GaussianProcess {
    /// Normalized training inputs.
    xs: Vec<[f64; 6]>,
    /// Per-dim normalization minima.
    lo: [f64; 6],
    /// Per-dim normalization spans (hi - lo, floored at 1.0).
    span: [f64; 6],
    /// Mean of the training targets; predictions are centered by it.
    y_mean: f64,
    /// Kernel weights: the Cholesky solve of (K + noise I) against centered targets.
    alpha: Vec<f64>,
    /// Packed lower-triangular Cholesky factor of (K + noise I).
    chol: Vec<f64>,
    /// Signal variance of the RBF kernel.
    signal_var: f64,
    /// Fitted lengthscale in normalized units.
    lengthscale: f64,
}

// CEP:ID: JIT-g31
// CEP:WHAT: Fits the GP on observed (features, latency) pairs.
// CEP:WHY: Phase 2's acquisition must be backed by all observations gathered so far, so the fit uses the most recent GP_MAX_FIT_POINTS points and derives the lengthscale from the median pairwise distance (deterministic, no hyperparameter search).
// CEP:STATUS: complete
// CEP:FAILURE: Returns None below GP_MIN_FIT_POINTS or when the kernel stays non-positive-definite after three jitter doublings; never panics.
// CEP:ASSUMES: targets are finite; duplicate points are allowed (the nugget keeps the kernel PD).
// CEP:COST: O(n^2) kernel build and median distance, O(n^3) Cholesky, capped by GP_MAX_FIT_POINTS.
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
/// Fits a GP on the observed (features, latency) pairs; None below the fit threshold or on a degenerate kernel.
pub fn fit(points: &[([f64; 6], f64)]) -> Option<GaussianProcess> {
    if points.len() < GP_MIN_FIT_POINTS {
        return None;
    }
    let pts = if points.len() > GP_MAX_FIT_POINTS {
        &points[points.len() - GP_MAX_FIT_POINTS..]
    } else {
        points
    };
    let n = pts.len();
    let mut lo = [f64::INFINITY; 6];
    let mut hi = [f64::NEG_INFINITY; 6];
    for (x, _) in pts {
        for d in 0..6 {
            lo[d] = lo[d].min(x[d]);
            hi[d] = hi[d].max(x[d]);
        }
    }
    let mut span = [0.0f64; 6];
    for d in 0..6 {
        span[d] = (hi[d] - lo[d]).max(1.0);
    }
    let norm = |x: &[f64; 6]| -> [f64; 6] {
        let mut v = [0.0f64; 6];
        for d in 0..6 {
            v[d] = (x[d] - lo[d]) / span[d];
        }
        v
    };
    let xs: Vec<[f64; 6]> = pts.iter().map(|(x, _)| norm(x)).collect();
    let y_mean = pts.iter().map(|(_, y)| y).sum::<f64>() / n as f64;
    let y_var = pts.iter().map(|(_, y)| (y - y_mean).powi(2)).sum::<f64>() / n as f64;

    // Median pairwise distance sets the lengthscale: half the points sit
    // within one correlation length of another, the rest stay uncertain.
    let mut dists: Vec<f64> = Vec::with_capacity(n * (n - 1) / 2);
    for i in 0..n {
        for j in (i + 1)..n {
            let mut d = 0.0;
            for k in 0..6 {
                d += (xs[i][k] - xs[j][k]).powi(2);
            }
            dists.push(d.sqrt());
        }
    }
    dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let lengthscale = dists
        .get(dists.len() / 2)
        .copied()
        .unwrap_or(1.0)
        .max(GP_LENGTHSCALE_FLOOR);
    let signal_var = (y_var.max(1e-12)).max(1.0);

    let noise = (GP_NOISE_REL * y_var).max(1e-9);
    let mut k = vec![0.0f64; n * (n + 1) / 2];
    for i in 0..n {
        for j in 0..=i {
            let mut d = 0.0;
            for c in 0..6 {
                d += (xs[i][c] - xs[j][c]).powi(2);
            }
            let cov = signal_var * (-d / (2.0 * lengthscale * lengthscale)).exp();
            k[i * (i + 1) / 2 + j] = cov + if i == j { noise } else { 0.0 };
        }
    }
    // Jitter retries: double the nugget until the factorization succeeds.
    let mut jitter_scale = 1.0f64;
    let chol = loop {
        let mut trial = k.clone();
        for i in 0..n {
            trial[i * (i + 1) / 2 + i] += noise * (jitter_scale - 1.0);
        }
        if cholesky_packed(&mut trial, n) {
            break trial;
        }
        jitter_scale *= 10.0;
        if jitter_scale > 1e3 {
            return None;
        }
    };
    let mut rhs: Vec<f64> = pts.iter().map(|(_, y)| y - y_mean).collect();
    chol_solve_packed(&chol, n, &mut rhs);
    Some(GaussianProcess {
        xs,
        lo,
        span,
        y_mean,
        alpha: rhs,
        chol,
        signal_var,
        lengthscale,
    })
}

impl GaussianProcess {
    /// Number of training points actually used.
    pub fn n_points(&self) -> usize {
        self.xs.len()
    }

    // CEP:ID: JIT-g32
    // CEP:WHAT: Predicts the posterior mean and variance at one candidate.
    // CEP:WHY: Expected Improvement needs both per-point terms; the variance is the kernel's self-covariance minus the explained part, clamped at zero.
    // CEP:STATUS: complete
    // CEP:FAILURE: none; variance is clamped at zero, all outputs finite for finite inputs.
    // CEP:ASSUMES: the model was fitted by fit().
    // CEP:COST: O(n) for the k* vector plus one triangular solve.
    // CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
    /// Predicts the posterior mean and variance at one candidate's features.
    pub fn predict(&self, x: &[f64; 6]) -> (f64, f64) {
        let mut v = [0.0f64; 6];
        for d in 0..6 {
            v[d] = (x[d] - self.lo[d]) / self.span[d];
        }
        let n = self.xs.len();
        let mut kstar = vec![0.0f64; n];
        for (i, xi) in self.xs.iter().enumerate() {
            let mut d = 0.0;
            for c in 0..6 {
                d += (xi[c] - v[c]).powi(2);
            }
            kstar[i] = self.signal_var * (-d / (2.0 * self.lengthscale * self.lengthscale)).exp();
        }
        let mu_delta: f64 = kstar
            .iter()
            .zip(self.alpha.iter())
            .map(|(k, a)| k * a)
            .sum();
        // v = L^-1 k*: forward substitution against the packed factor.
        let mut w = kstar.clone();
        for i in 0..n {
            let row = i * (i + 1) / 2;
            let mut s = w[i];
            for j in 0..i {
                s -= self.chol[row + j] * w[j];
            }
            w[i] = s / self.chol[row + i];
        }
        let explained: f64 = w.iter().map(|x| x * x).sum();
        (
            self.y_mean + mu_delta,
            (self.signal_var - explained).max(0.0),
        )
    }
}

// CEP:ID: JIT-g33
// CEP:WHAT: Computes the Expected Improvement of one posterior at the incumbent best.
// CEP:WHY: EI is the one-step Bayesian-optimal acquisition for minimization: it rises with both promise (low mean) and uncertainty (high sigma), which is exactly the Phase 2 exploration-exploitation balance the design names.
// CEP:STATUS: complete
// CEP:FAILURE: none; sigma is floored so the division stays finite.
// CEP:ASSUMES: latencies are minimized; best is the incumbent's measured latency.
// CEP:COST: O(1); the error function is the Abramowitz-Stegun 7.1.26 rational approximation (absolute error below 1.5e-7).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
/// Expected Improvement of one posterior at the incumbent best (minimization).
pub fn expected_improvement(mu: f64, sigma: f64, best: f64) -> f64 {
    let sigma = sigma.max(1e-9);
    let z = (best - mu) / sigma;
    let phi = (-0.5 * z * z).exp() / (2.0 * std::f64::consts::PI).sqrt();
    let phi_cdf = 0.5 * (1.0 + erf(z / std::f64::consts::SQRT_2));
    (best - mu) * phi_cdf + sigma * phi
}

// CEP:ID: JIT-g34
// CEP:WHAT: The Abramowitz-Stegun 7.1.26 error-function approximation.
// CEP:WHY: std has no erf; the rational approximation is deterministic, allocation-free, and accurate enough for acquisition ranking.
// CEP:STATUS: complete
// CEP:FAILURE: none; monotone odd function, clamped to [-1, 1] outside the fit's |x| <= 3.5 domain.
// CEP:ASSUMES: none
// CEP:COST: O(1).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
fn erf(x: f64) -> f64 {
    let a1: f64 = 0.254829592;
    let a2: f64 = -0.284496736;
    let a3: f64 = 1.421413741;
    let a4: f64 = -1.453152027;
    let a5: f64 = 1.061405429;
    let p: f64 = 0.3275911;
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + p * x);
    let y = 1.0 - ((((a5 * t + a4) * t + a3) * t + a2) * t + a1) * t * (-x * x).exp();
    sign * y.clamp(-1.0, 1.0)
}

// CEP:ID: JIT-g35
// CEP:WHAT: In-place Cholesky factorization of a packed symmetric positive-definite matrix.
// CEP:WHY: The GP solve needs one factorization reused for all per-candidate predictions; packed storage keeps the fit allocation to one buffer.
// CEP:STATUS: complete
// CEP:FAILURE: Returns false when a diagonal pivot is non-positive (non-PD input); the input is then unspecified.
// CEP:ASSUMES: a holds the lower triangle row-major, row i at i*(i+1)/2.
// CEP:COST: O(n^3).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
fn cholesky_packed(a: &mut [f64], n: usize) -> bool {
    for i in 0..n {
        for j in 0..=i {
            let row_i = i * (i + 1) / 2;
            let row_j = j * (j + 1) / 2;
            let mut s = a[row_i + j];
            for k in 0..j {
                s -= a[row_i + k] * a[row_j + k];
            }
            if i == j {
                if s <= 1e-12 {
                    return false;
                }
                a[row_i + i] = s.sqrt();
            } else {
                a[row_i + j] = s / a[row_j + j];
            }
        }
    }
    true
}

// CEP:ID: JIT-g36
// CEP:WHAT: Solves L L^T x = b in place against a packed Cholesky factor.
// CEP:WHY: Both the fit (alpha) and predictions reuse the same factorization, so one triangular pair serves both paths.
// CEP:STATUS: complete
// CEP:FAILURE: none; the caller guarantees a successful factorization.
// CEP:ASSUMES: chol came from a successful cholesky_packed on the same n.
// CEP:COST: O(n^2).
// CEP:EVIDENCE: crates/mlkp-autotune/tests/autotune_test.rs
fn chol_solve_packed(chol: &[f64], n: usize, b: &mut [f64]) {
    // Forward: L w = b.
    for i in 0..n {
        let row = i * (i + 1) / 2;
        let mut s = b[i];
        for j in 0..i {
            s -= chol[row + j] * b[j];
        }
        b[i] = s / chol[row + i];
    }
    // Backward: L^T x = w.
    for i in (0..n).rev() {
        let mut s = b[i];
        for (jj, &bj) in b[(i + 1)..n].iter().enumerate() {
            let j = i + 1 + jj;
            let row_j = j * (j + 1) / 2;
            s -= chol[row_j + i] * bj;
        }
        b[i] = s / chol[i * (i + 1) / 2 + i];
    }
}
