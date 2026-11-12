// CEP:FILE: crates/mlkp-runtime/src/timing.rs
// CEP:WHAT: Deterministic measurement discipline: warmup-discard, median-of-N latency, variance tracking, and the confidence-interval policy of the final polish phase.
// CEP:WHY: Design section VII names measurement noise as a failure mode and the final polish phase must apply a statistical policy instead of trusting a mean; this module is where the design's measurement rules become code.
// CEP:CLASS: CEP-1
// CEP:STATUS: complete
// CEP:FAILURE: none; measurement is total over any closure.
// CEP:ASSUMES: the closure under measurement is deterministic in output; wall-clock noise is handled by the median and variance, not by the closure.
// CEP:COST: cold; the closure runs warmup + runs times.
// CEP:EVIDENCE: unit tests in crates/mlkp-runtime/tests/timing_test.rs.

#![deny(missing_docs)]

use std::time::Duration;

/// One measured series.
#[derive(Clone, Copy, Debug)]
pub struct Measurement {
    /// Median duration.
    pub median: Duration,
    /// Minimum duration (lower bound of the noise).
    pub min: Duration,
    /// Arithmetic mean duration.
    pub mean: Duration,
    /// Sample variance of the series.
    pub variance_ns2: f64,
    /// Number of timed runs.
    pub runs: u32,
}

impl Measurement {
    /// Relative noise estimate: sqrt(variance) / mean.
    // CEP:WHAT: Computes the relative noise of the series.
    // CEP:WHY: The noise policy extends measurement when two candidates overlap within noise.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns 0.0 when the mean is zero to avoid division by zero.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/timing_test.rs
    pub fn relative_noise(&self) -> f64 {
        let mean_ns = self.mean.as_nanos() as f64;
        if mean_ns <= 0.0 {
            return 0.0;
        }
        self.variance_ns2.sqrt() / mean_ns
    }

    /// Welch-style t statistic for two independent series with unequal variances.
    // CEP:WHAT: Computes the Welch t statistic between two series.
    // CEP:WHY: The final polish phase compares the top candidates with a t-test rather than raw means (design fix 4).
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns 0.0 when either variance is zero.
    // CEP:ASSUMES: both series have at least two runs.
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/timing_test.rs
    pub fn welch_t(a: &Measurement, b: &Measurement) -> f64 {
        let ma = a.mean.as_nanos() as f64;
        let mb = b.mean.as_nanos() as f64;
        let va = a.variance_ns2;
        let vb = b.variance_ns2;
        let na = f64::from(a.runs.max(1));
        let nb = f64::from(b.runs.max(1));
        let denom = (va / na + vb / nb).sqrt();
        if denom <= 0.0 {
            return 0.0;
        }
        (ma - mb) / denom
    }

    /// Approximate degrees of freedom for the Welch t statistic (Welch-Satterthwaite).
    // CEP:WHAT: Computes Welch degrees of freedom.
    // CEP:WHY: The t-test needs df to compare against the critical value.
    // CEP:STATUS: complete
    // CEP:FAILURE: Returns 1.0 when the denominator degenerates.
    // CEP:ASSUMES: none
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/timing_test.rs
    pub fn welch_df(a: &Measurement, b: &Measurement) -> f64 {
        let va = a.variance_ns2;
        let vb = b.variance_ns2;
        let na = f64::from(a.runs.max(1));
        let nb = f64::from(b.runs.max(1));
        let num = (va / na + vb / nb).powi(2);
        let den = (va / na).powi(2) / (na - 1.0).max(1.0) + (vb / nb).powi(2) / (nb - 1.0).max(1.0);
        if den <= 0.0 {
            return 1.0;
        }
        num / den
    }

    /// Whether two series are statistically distinguishable at the configured confidence.
    // CEP:WHAT: Applies the confidence policy.
    // CEP:WHY: The polish phase must pick the lower-variance candidate when intervals overlap (design fix 4).
    // CEP:STATUS: complete
    // CEP:FAILURE: none
    // CEP:ASSUMES: the two-tailed critical value for the normal approximation bounds the t distribution conservatively at 95 percent and 99 percent levels.
    // CEP:COST: O(1).
    // CEP:EVIDENCE: crates/mlkp-runtime/tests/timing_test.rs
    pub fn distinguishable(a: &Measurement, b: &Measurement, confidence: f64) -> bool {
        let t = Measurement::welch_t(a, b).abs();
        // Conservative two-tailed normal-approximation critical values.
        let critical = if confidence >= 0.99 { 2.576 } else { 1.96 };
        t > critical
    }
}

/// Measures a closure with warmup discard and a median-of-N summary.
// CEP:WHAT: Measures one closure.
// CEP:WHY: Every candidate evaluation and every bench artifact flows through this single discipline.
// CEP:STATUS: complete
// CEP:FAILURE: none
// CEP:ASSUMES: the closure is deterministic in output; each run must leave the arena so the next run measures the same work.
// CEP:COST: O(runs * closure).
// CEP:EVIDENCE: crates/mlkp-runtime/tests/timing_test.rs
pub fn measure<F: FnMut() -> Result<(), E>, E>(
    mut f: F,
    warmup: u32,
    runs: u32,
) -> Result<Measurement, E> {
    for _ in 0..warmup {
        f()?;
    }
    let mut durations: Vec<Duration> = Vec::with_capacity(runs as usize);
    for _ in 0..runs {
        let start = std::time::Instant::now();
        f()?;
        durations.push(start.elapsed());
    }
    durations.sort();
    let median = durations
        .get((runs as usize) / 2)
        .copied()
        .unwrap_or_default();
    let min = durations.first().copied().unwrap_or_default();
    let total: u128 = durations.iter().map(|d| d.as_nanos()).sum();
    let mean = Duration::from_nanos((total / u128::from(runs.max(1))) as u64);
    let mean_ns = mean.as_nanos() as f64;
    let variance = if durations.len() > 1 {
        let n = durations.len() as f64;
        durations
            .iter()
            .map(|d| {
                let x = d.as_nanos() as f64 - mean_ns;
                x * x
            })
            .sum::<f64>()
            / (n - 1.0)
    } else {
        0.0
    };
    Ok(Measurement {
        median,
        min,
        mean,
        variance_ns2: variance,
        runs,
    })
}
