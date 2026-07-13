//! Online variance using Welford's algorithm.
//!
//! Time complexity per update: `O(1)`. Space complexity: `O(1)`.
//!
//! See <https://en.wikipedia.org/wiki/Algorithms_for_calculating_variance#Welford's_online_algorithm>.

use crate::error::{RillError, ensure_finite};
use crate::traits::OnlineStatistic;

/// Whether to compute population or sample variance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum VarianceKind {
    /// Divide by `n`. This is the population (biased) variance.
    Population,
    /// Divide by `n - 1`. This is the sample (Bessel-corrected) variance.
    #[default]
    Sample,
}

impl VarianceKind {
    /// Returns the denominator for the given number of observations.
    fn denominator(self, n: u64) -> Option<u64> {
        match self {
            VarianceKind::Population => {
                if n == 0 {
                    None
                } else {
                    Some(n)
                }
            }
            VarianceKind::Sample => {
                if n < 2 {
                    None
                } else {
                    Some(n - 1)
                }
            }
        }
    }
}

/// Online variance accumulator using Welford's algorithm.
///
/// Also exposes the running mean and population/sample standard deviation.
///
/// # Examples
///
/// ```
/// use rill_ml::stats::{Variance, VarianceKind};
/// use rill_ml::OnlineStatistic;
///
/// let mut v = Variance::new(VarianceKind::Population);
/// for x in [1.0, 2.0, 3.0, 4.0, 5.0] {
///     v.update(x).unwrap();
/// }
/// assert!((v.value().unwrap() - 2.0).abs() < 1e-12);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Variance {
    count: u64,
    mean: f64,
    m2: f64,
    kind: VarianceKind,
}

impl Variance {
    /// Create a new variance accumulator of the given kind.
    pub const fn new(kind: VarianceKind) -> Self {
        Self {
            count: 0,
            mean: 0.0,
            m2: 0.0,
            kind,
        }
    }

    /// Current variance, or `None` when not enough data has been observed.
    pub fn value(&self) -> Option<f64> {
        self.kind
            .denominator(self.count)
            .map(|denom| self.m2 / denom as f64)
    }

    /// Current standard deviation, or `None` when not enough data has been observed.
    pub fn std_dev(&self) -> Option<f64> {
        self.value().map(|v| v.sqrt())
    }

    /// Current running mean.
    pub const fn mean(&self) -> f64 {
        self.mean
    }

    /// Number of observations seen so far.
    pub const fn count(&self) -> u64 {
        self.count
    }

    /// The configured variance kind.
    pub const fn kind(&self) -> VarianceKind {
        self.kind
    }
}

impl OnlineStatistic for Variance {
    fn update(&mut self, value: f64) -> Result<(), RillError> {
        ensure_finite("value", value)?;
        self.count += 1;
        let n = self.count as f64;
        let delta = value - self.mean;
        self.mean += delta / n;
        let delta2 = value - self.mean;
        self.m2 += delta * delta2;
        Ok(())
    }

    fn samples_seen(&self) -> u64 {
        self.count
    }

    fn reset(&mut self) {
        self.count = 0;
        self.mean = 0.0;
        self.m2 = 0.0;
    }
}

impl Default for Variance {
    fn default() -> Self {
        Self::new(VarianceKind::Sample)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn population_variance_of_simple_sequence() {
        let mut v = Variance::new(VarianceKind::Population);
        for x in [1.0, 2.0, 3.0, 4.0, 5.0] {
            v.update(x).unwrap();
        }
        assert!((v.value().unwrap() - 2.0).abs() < 1e-12);
        assert!((v.std_dev().unwrap() - 2.0_f64.sqrt()).abs() < 1e-12);
        assert!((v.mean() - 3.0).abs() < 1e-12);
    }

    #[test]
    fn sample_variance_of_simple_sequence() {
        let mut v = Variance::new(VarianceKind::Sample);
        for x in [1.0, 2.0, 3.0, 4.0, 5.0] {
            v.update(x).unwrap();
        }
        // sample variance = 10 / 4 = 2.5
        assert!((v.value().unwrap() - 2.5).abs() < 1e-12);
    }

    #[test]
    fn variance_insufficient_data_returns_none() {
        let pop = Variance::new(VarianceKind::Population);
        assert!(pop.value().is_none());

        let mut sample = Variance::new(VarianceKind::Sample);
        sample.update(5.0).unwrap();
        assert!(sample.value().is_none());
    }

    #[test]
    fn variance_constant_sequence_is_zero() {
        let mut v = Variance::new(VarianceKind::Population);
        for _ in 0..100 {
            v.update(7.0).unwrap();
        }
        assert_eq!(v.value().unwrap(), 0.0);
    }

    #[test]
    fn variance_rejects_non_finite() {
        let mut v = Variance::new(VarianceKind::Population);
        assert!(v.update(f64::NAN).is_err());
        assert_eq!(v.count(), 0);
    }

    #[test]
    fn variance_matches_batch_formula() {
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(99);
        let mut v = Variance::new(VarianceKind::Population);
        let mut data = Vec::new();
        for _ in 0..2000 {
            let x = rand::Rng::gen_range(&mut rng, -50.0..50.0);
            v.update(x).unwrap();
            data.push(x);
        }
        let mean = data.iter().sum::<f64>() / data.len() as f64;
        let pop_var = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / data.len() as f64;
        assert!(
            (v.value().unwrap() - pop_var).abs() < 1e-6,
            "online vs batch variance mismatch"
        );
    }
}
