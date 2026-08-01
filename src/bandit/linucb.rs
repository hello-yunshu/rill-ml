//! LinUCB contextual bandit algorithm.
//!
//! LinUCB extends the multi-armed bandit to contextual settings by maintaining
//! a linear model for each arm. For a context vector `x`, each arm `a` is
//! scored by:
//!
//! ```text
//! p_a = theta_a^T * x + alpha * sqrt(x^T * A_a^{-1} * x)
//! ```
//!
//! where `A_a` is the `d x d` matrix `I + sum(x_t * x_t^T)` over observed
//! updates for arm `a`, `b_a` is the `d` vector `sum(reward_t * x_t)`, and
//! `theta_a = A_a^{-1} * b_a`. The first term exploits the arm's linear model;
//! the second term is an exploration bonus that is large for under-explored
//! arms (in directions where `A_a^{-1}` is still big).
//!
//! On `update`, `A_a += x * x^T` and `b_a += reward * x`.
//!
//! ## Complexity
//!
//! - `select`: `O(arm_count * d^3)` — a Cholesky factorisation and two
//!   triangular solves per arm. This avoids explicitly forming a matrix
//!   inverse and is numerically safer for the symmetric positive-definite
//!   ridge matrices maintained by LinUCB.
//! - `update`: `O(d^2)` for the outer-product accumulation on the selected arm
//!   (other arms are untouched).
//! - Space: `O(arm_count * d^2)`.
//!
//! ## Reference
//!
//! Li, Chu, Langford, Schapire. "A Contextual-Bandit Approach to Personalized
//! News Article Recommendation." WWW 2010.

use crate::bandit::{
    ContextualBandit, checked_finite_add, checked_increment, validate_arm, validate_reward_finite,
};
use crate::error::{RillError, ensure_finite};
use crate::persistence::ValidateState;
use rand::Rng;

/// Configuration for [`LinUcb`].
///
/// # Examples
///
/// ```
/// use rill_ml::bandit::LinUcbConfig;
///
/// let mut config = LinUcbConfig::default();
/// config.alpha = 1.0;
/// config.arm_count = 3;
/// config.feature_count = 2;
/// assert_eq!(config.arm_count, 3);
/// ```
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct LinUcbConfig {
    /// Exploration parameter `alpha`. Controls the exploration/exploitation
    /// trade-off. Higher values favor exploration. Must be finite and positive.
    ///
    /// The original paper suggests `alpha = 1.0` as a reasonable default.
    pub alpha: f64,
    /// Number of arms (actions). Must be greater than zero.
    pub arm_count: usize,
    /// Number of features in the context vector. Must be greater than zero.
    pub feature_count: usize,
}

impl Default for LinUcbConfig {
    fn default() -> Self {
        Self {
            alpha: 1.0,
            arm_count: 2,
            feature_count: 1,
        }
    }
}

impl LinUcbConfig {
    /// Validate the configuration without constructing a bandit.
    pub fn validate(&self) -> Result<(), RillError> {
        if self.arm_count == 0 {
            return Err(RillError::InvalidArmCount(self.arm_count));
        }
        if self.feature_count == 0 {
            return Err(RillError::InvalidFeatureCount(self.feature_count));
        }
        if !self.alpha.is_finite() || self.alpha <= 0.0 {
            return Err(RillError::InvalidParameter {
                name: "alpha",
                value: self.alpha,
            });
        }
        Ok(())
    }
}

/// Explainable score components for one LinUCB arm.
///
/// [`exploration_bonus`](Self::exploration_bonus) already includes the
/// configured `alpha` multiplier, so
/// `total_score = exploitation + exploration_bonus`.
#[derive(Debug, Clone, PartialEq)]
pub struct LinUcbArmScore {
    /// Zero-based arm index.
    pub arm: usize,
    /// Estimated reward `theta_a^T x`.
    pub exploitation: f64,
    /// Confidence bonus `alpha * sqrt(x^T A_a^-1 x)`.
    pub exploration_bonus: f64,
    /// The exact score used by selection.
    pub total_score: f64,
}

/// Numerical diagnostics for one LinUCB arm's ridge matrix.
#[derive(Debug, Clone, PartialEq)]
pub struct LinUcbConditionDiagnostics {
    /// Zero-based arm index.
    pub arm: usize,
    /// Smallest diagonal entry of the Cholesky factor.
    pub min_cholesky_diagonal: f64,
    /// Largest diagonal entry of the Cholesky factor.
    pub max_cholesky_diagonal: f64,
    /// A cheap condition indicator `(max_diag / min_diag)^2`.
    ///
    /// This is not an exact matrix condition number, but a large value is a
    /// useful signal that the arm state is becoming poorly conditioned.
    pub condition_indicator: f64,
}

/// LinUCB contextual multi-armed bandit.
///
/// Maintains a per-arm ridge-regression model and selects the arm with the
/// highest upper confidence bound on the expected reward for the given
/// context.
///
/// # Examples
///
/// ```
/// use rill_ml::bandit::{ContextualBandit, LinUcb, LinUcbConfig};
/// use rand::SeedableRng;
/// use rand_chacha::ChaCha8Rng;
///
/// let mut config = LinUcbConfig::default();
/// config.alpha = 1.0;
/// config.arm_count = 2;
/// config.feature_count = 2;
/// let mut bandit = LinUcb::new(config).unwrap();
/// let mut rng = ChaCha8Rng::seed_from_u64(0);
///
/// let context = [0.5, 0.8];
/// let arm = bandit.select(&context, &mut rng).unwrap();
/// bandit.update(arm, &context, 1.0).unwrap();
/// assert_eq!(bandit.samples_seen(), 1);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LinUcb {
    arm_count: usize,
    feature_count: usize,
    alpha: f64,
    /// Per-arm `d x d` matrices `A_a`, initialized to the identity matrix.
    a_matrices: Vec<Vec<Vec<f64>>>,
    /// Per-arm `d` vectors `b_a`, initialized to zero.
    b_vectors: Vec<Vec<f64>>,
    /// Total number of updates.
    samples_seen: u64,
}

impl LinUcb {
    /// Create a new LinUCB bandit from the given configuration.
    ///
    /// # Errors
    ///
    /// Returns `RillError::InvalidArmCount` if `arm_count` is zero.
    /// Returns `RillError::InvalidFeatureCount` if `feature_count` is zero.
    /// Returns `RillError::InvalidParameter` if `alpha` is not finite and
    /// positive.
    pub fn new(config: LinUcbConfig) -> Result<Self, RillError> {
        config.validate()?;

        let d = config.feature_count;
        let a_matrices = (0..config.arm_count).map(|_| identity_matrix(d)).collect();
        let b_vectors = (0..config.arm_count).map(|_| vec![0.0; d]).collect();

        Ok(Self {
            arm_count: config.arm_count,
            feature_count: config.feature_count,
            alpha: config.alpha,
            a_matrices,
            b_vectors,
            samples_seen: 0,
        })
    }

    /// The exploration parameter `alpha`.
    pub const fn alpha(&self) -> f64 {
        self.alpha
    }

    /// Borrow the `A` matrix for a specific arm (diagnostic).
    ///
    /// # Errors
    ///
    /// Returns `RillError::InvalidArm` if `arm` is out of range.
    pub fn a_matrix(&self, arm: usize) -> Result<&[Vec<f64>], RillError> {
        validate_arm(self.arm_count, arm)?;
        Ok(&self.a_matrices[arm])
    }

    /// Borrow the `b` vector for a specific arm (diagnostic).
    ///
    /// # Errors
    ///
    /// Returns `RillError::InvalidArm` if `arm` is out of range.
    pub fn b_vector(&self, arm: usize) -> Result<&[f64], RillError> {
        validate_arm(self.arm_count, arm)?;
        Ok(&self.b_vectors[arm])
    }

    /// Validate all persisted state invariants.
    ///
    /// This is also run automatically during deserialization.
    pub fn validate(&self) -> Result<(), RillError> {
        LinUcbConfig {
            alpha: self.alpha,
            arm_count: self.arm_count,
            feature_count: self.feature_count,
        }
        .validate()?;
        if self.a_matrices.len() != self.arm_count || self.b_vectors.len() != self.arm_count {
            return Err(RillError::InvalidState(
                "arm_count does not match per-arm state lengths".to_owned(),
            ));
        }

        for arm in 0..self.arm_count {
            let matrix = &self.a_matrices[arm];
            let vector = &self.b_vectors[arm];
            if matrix.len() != self.feature_count
                || matrix.iter().any(|row| row.len() != self.feature_count)
                || vector.len() != self.feature_count
            {
                return Err(RillError::InvalidState(format!(
                    "arm {arm} state does not match feature_count"
                )));
            }
            if matrix.iter().flatten().any(|value| !value.is_finite())
                || vector.iter().any(|value| !value.is_finite())
            {
                return Err(RillError::InvalidState(format!(
                    "arm {arm} state contains a non-finite value"
                )));
            }
            for (i, row) in matrix.iter().enumerate() {
                for (j, &value) in row.iter().take(i).enumerate() {
                    if value != matrix[j][i] {
                        return Err(RillError::InvalidState(format!(
                            "A matrix for arm {arm} is not symmetric"
                        )));
                    }
                }
            }
            if !matrix_is_positive_definite(matrix) {
                return Err(RillError::InvalidState(format!(
                    "A matrix for arm {arm} is not positive definite"
                )));
            }
        }
        Ok(())
    }

    /// Validate that the context vector has the expected length and is finite.
    fn validate_context(&self, context: &[f64]) -> Result<(), RillError> {
        if context.len() != self.feature_count {
            return Err(RillError::DimensionMismatch {
                expected: self.feature_count,
                actual: context.len(),
            });
        }
        for (i, &v) in context.iter().enumerate() {
            if !v.is_finite() {
                return Err(RillError::NonFiniteValue {
                    field: "context",
                    value: context[i],
                });
            }
        }
        Ok(())
    }

    /// Compute the explainable UCB score for one arm.
    ///
    /// The returned exploration bonus already includes `alpha`.
    pub fn score_arm(&self, arm: usize, context: &[f64]) -> Result<LinUcbArmScore, RillError> {
        validate_arm(self.arm_count, arm)?;
        self.validate_context(context)?;
        self.score_arm_validated(arm, context)
    }

    /// Compute explainable UCB scores for every arm.
    pub fn score_all(&self, context: &[f64]) -> Result<Vec<LinUcbArmScore>, RillError> {
        self.validate_context(context)?;
        (0..self.arm_count)
            .map(|arm| self.score_arm_validated(arm, context))
            .collect()
    }

    /// Select an arm with the existing randomized tie-break and return every
    /// score used by that decision.
    pub fn select_with_scores(
        &self,
        context: &[f64],
        rng: &mut impl Rng,
    ) -> Result<(usize, Vec<LinUcbArmScore>), RillError> {
        let scores = self.score_all(context)?;
        let arm = select_random_tie(&scores, rng);
        Ok((arm, scores))
    }

    /// Select deterministically, resolving exact score ties to the lowest arm
    /// index. This does not change the randomized [`ContextualBandit::select`]
    /// contract and is intended for replay and audit paths.
    pub fn select_deterministic(&self, context: &[f64]) -> Result<usize, RillError> {
        self.validate_context(context)?;
        let mut best_arm = 0usize;
        let mut best_score = f64::NEG_INFINITY;
        for arm in 0..self.arm_count {
            let score = self.score_arm_validated(arm, context)?.total_score;
            if score > best_score {
                best_score = score;
                best_arm = arm;
            }
        }
        Ok(best_arm)
    }

    /// Return a bounded numerical condition diagnostic for one arm.
    pub fn condition_diagnostics(
        &self,
        arm: usize,
    ) -> Result<LinUcbConditionDiagnostics, RillError> {
        validate_arm(self.arm_count, arm)?;
        let lower = cholesky_factor(&self.a_matrices[arm])?;
        let mut min_diagonal = f64::INFINITY;
        let mut max_diagonal = 0.0_f64;
        for (i, row) in lower.iter().enumerate() {
            min_diagonal = min_diagonal.min(row[i]);
            max_diagonal = max_diagonal.max(row[i]);
        }
        let ratio = max_diagonal / min_diagonal;
        let condition_indicator = ratio * ratio;
        if !condition_indicator.is_finite() {
            return Err(RillError::InvalidState(
                "LinUCB condition indicator is non-finite".to_owned(),
            ));
        }
        Ok(LinUcbConditionDiagnostics {
            arm,
            min_cholesky_diagonal: min_diagonal,
            max_cholesky_diagonal: max_diagonal,
            condition_indicator,
        })
    }

    fn score_arm_validated(
        &self,
        arm: usize,
        context: &[f64],
    ) -> Result<LinUcbArmScore, RillError> {
        let lower = cholesky_factor(&self.a_matrices[arm])?;
        let b = &self.b_vectors[arm];
        // Solve A * theta = b without explicitly forming A^-1.
        let theta = cholesky_solve(&lower, b)?;
        // theta^T * x
        let exploitation = checked_dot(&theta, context, "LinUCB exploitation")?;
        // Solve A * z = x, then compute x^T z.
        let solved_context = cholesky_solve(&lower, context)?;
        let quad = checked_dot(context, &solved_context, "LinUCB exploration variance")?;
        // Numerical safety: the quadratic form should be non-negative for a
        // positive-definite A, but rounding can make it slightly negative.
        let quad_safe = if quad < 0.0 { 0.0 } else { quad };
        let exploration_bonus = self.alpha * quad_safe.sqrt();
        if !exploration_bonus.is_finite() {
            return Err(RillError::NonFiniteValue {
                field: "LinUCB exploration bonus",
                value: exploration_bonus,
            });
        }
        let total_score =
            checked_finite_add(exploitation, exploration_bonus, "LinUCB total score")?;
        Ok(LinUcbArmScore {
            arm,
            exploitation,
            exploration_bonus,
            total_score,
        })
    }
}

impl ContextualBandit for LinUcb {
    fn arm_count(&self) -> usize {
        self.arm_count
    }

    fn feature_count(&self) -> usize {
        self.feature_count
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn select(&self, context: &[f64], rng: &mut impl Rng) -> Result<usize, RillError> {
        self.validate_context(context)?;
        let mut best_arm = 0usize;
        let mut best_score = f64::NEG_INFINITY;
        let mut tied = 0usize;
        for arm in 0..self.arm_count {
            let score = self.score_arm_validated(arm, context)?.total_score;
            if score > best_score {
                best_score = score;
                best_arm = arm;
                tied = 1;
            } else if score == best_score {
                tied += 1;
                if rng.gen_range(0..tied) == 0 {
                    best_arm = arm;
                }
            }
        }
        Ok(best_arm)
    }

    fn update(&mut self, arm: usize, context: &[f64], reward: f64) -> Result<(), RillError> {
        validate_arm(self.arm_count, arm)?;
        self.validate_context(context)?;
        validate_reward_finite(reward)?;

        let d = self.feature_count;
        let mut next_a = self.a_matrices[arm].clone();
        for i in 0..d {
            for j in 0..d {
                next_a[i][j] =
                    checked_finite_add(next_a[i][j], context[i] * context[j], "A matrix")?;
            }
        }
        let mut next_b = self.b_vectors[arm].clone();
        for i in 0..d {
            next_b[i] = checked_finite_add(next_b[i], reward * context[i], "b vector")?;
        }
        let next_samples = checked_increment(self.samples_seen, "samples_seen")?;
        // A finite symmetric update can still become numerically unusable at
        // extreme scales. Reject before commit so scoring never observes a
        // non-positive-definite arm state.
        cholesky_factor(&next_a)?;

        self.a_matrices[arm] = next_a;
        self.b_vectors[arm] = next_b;
        self.samples_seen = next_samples;
        Ok(())
    }

    fn reset(&mut self) {
        for a in &mut self.a_matrices {
            *a = identity_matrix(self.feature_count);
        }
        for b in &mut self.b_vectors {
            for v in b.iter_mut() {
                *v = 0.0;
            }
        }
        self.samples_seen = 0;
    }
}

/// Preview high-performance LinUCB implementation.
///
/// `LinUcbFast` maintains `A^-1` directly with the Sherman-Morrison rank-one
/// update. Selection and update are `O(arm_count * d^2)` and `O(d^2)`
/// respectively. Its serde state is Preview and is not part of the frozen
/// [`LinUcb`] state schema.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct LinUcbFast {
    arm_count: usize,
    feature_count: usize,
    alpha: f64,
    inverse_matrices: Vec<Vec<Vec<f64>>>,
    b_vectors: Vec<Vec<f64>>,
    samples_seen: u64,
}

impl LinUcbFast {
    /// Create an empty fast LinUCB from the same configuration as [`LinUcb`].
    pub fn new(config: LinUcbConfig) -> Result<Self, RillError> {
        config.validate()?;
        let inverse_matrices = (0..config.arm_count)
            .map(|_| identity_matrix(config.feature_count))
            .collect();
        let b_vectors = (0..config.arm_count)
            .map(|_| vec![0.0; config.feature_count])
            .collect();
        Ok(Self {
            arm_count: config.arm_count,
            feature_count: config.feature_count,
            alpha: config.alpha,
            inverse_matrices,
            b_vectors,
            samples_seen: 0,
        })
    }

    /// Convert a stable LinUCB state into the Preview fast representation.
    pub fn from_linucb(source: &LinUcb) -> Result<Self, RillError> {
        source.validate()?;
        let mut inverse_matrices = Vec::with_capacity(source.arm_count);
        for matrix in &source.a_matrices {
            let lower = cholesky_factor(matrix)?;
            inverse_matrices.push(inverse_from_cholesky(&lower)?);
        }
        let fast = Self {
            arm_count: source.arm_count,
            feature_count: source.feature_count,
            alpha: source.alpha,
            inverse_matrices,
            b_vectors: source.b_vectors.clone(),
            samples_seen: source.samples_seen,
        };
        fast.validate()?;
        Ok(fast)
    }

    /// Borrow one cached inverse matrix.
    pub fn inverse_matrix(&self, arm: usize) -> Result<&[Vec<f64>], RillError> {
        validate_arm(self.arm_count, arm)?;
        Ok(&self.inverse_matrices[arm])
    }

    /// Number of stored `f64` values, excluding allocator metadata.
    pub const fn state_f64_count(&self) -> usize {
        self.arm_count * (self.feature_count * self.feature_count + self.feature_count) + 1
    }

    /// Explain one arm score in `O(d^2)`.
    pub fn score_arm(&self, arm: usize, context: &[f64]) -> Result<LinUcbArmScore, RillError> {
        validate_arm(self.arm_count, arm)?;
        self.validate_context(context)?;
        self.score_arm_validated(arm, context)
    }

    /// Explain all arm scores in `O(arm_count * d^2)`.
    pub fn score_all(&self, context: &[f64]) -> Result<Vec<LinUcbArmScore>, RillError> {
        self.validate_context(context)?;
        (0..self.arm_count)
            .map(|arm| self.score_arm_validated(arm, context))
            .collect()
    }

    /// Deterministic lowest-index tie-break for replay and audit.
    pub fn select_deterministic(&self, context: &[f64]) -> Result<usize, RillError> {
        self.validate_context(context)?;
        let mut best_arm = 0;
        let mut best_score = f64::NEG_INFINITY;
        for arm in 0..self.arm_count {
            let score = self.score_arm_validated(arm, context)?.total_score;
            if score > best_score {
                best_score = score;
                best_arm = arm;
            }
        }
        Ok(best_arm)
    }

    /// Validate dimensions, finite values, symmetry and positive definiteness.
    pub fn validate(&self) -> Result<(), RillError> {
        LinUcbConfig {
            alpha: self.alpha,
            arm_count: self.arm_count,
            feature_count: self.feature_count,
        }
        .validate()?;
        if self.inverse_matrices.len() != self.arm_count || self.b_vectors.len() != self.arm_count {
            return Err(RillError::InvalidState(
                "fast LinUCB arm state lengths are inconsistent".to_owned(),
            ));
        }
        for arm in 0..self.arm_count {
            let matrix = &self.inverse_matrices[arm];
            let vector = &self.b_vectors[arm];
            if matrix.len() != self.feature_count
                || matrix.iter().any(|row| row.len() != self.feature_count)
                || vector.len() != self.feature_count
                || matrix.iter().flatten().any(|value| !value.is_finite())
                || vector.iter().any(|value| !value.is_finite())
            {
                return Err(RillError::InvalidState(format!(
                    "fast LinUCB arm {arm} has malformed dimensions or values"
                )));
            }
            for (i, row) in matrix.iter().enumerate() {
                for (j, &lower_value) in row.iter().take(i).enumerate() {
                    let upper_value = matrix[j][i];
                    let scale = lower_value.abs().max(upper_value.abs()).max(1.0);
                    if (lower_value - upper_value).abs() > 1e-12 * scale {
                        return Err(RillError::InvalidState(format!(
                            "fast LinUCB inverse for arm {arm} is not symmetric"
                        )));
                    }
                }
            }
            cholesky_factor(matrix)?;
        }
        Ok(())
    }

    fn validate_context(&self, context: &[f64]) -> Result<(), RillError> {
        if context.len() != self.feature_count {
            return Err(RillError::DimensionMismatch {
                expected: self.feature_count,
                actual: context.len(),
            });
        }
        for &value in context {
            ensure_finite("context", value)?;
        }
        Ok(())
    }

    fn score_arm_validated(
        &self,
        arm: usize,
        context: &[f64],
    ) -> Result<LinUcbArmScore, RillError> {
        let inverse = &self.inverse_matrices[arm];
        let theta = checked_matrix_vector_mul(inverse, &self.b_vectors[arm])?;
        let exploitation = checked_dot(&theta, context, "fast LinUCB exploitation")?;
        let solved_context = checked_matrix_vector_mul(inverse, context)?;
        let variance = checked_dot(context, &solved_context, "fast LinUCB variance")?;
        let exploration_bonus = self.alpha * variance.max(0.0).sqrt();
        ensure_finite("fast LinUCB exploration bonus", exploration_bonus)?;
        let total_score =
            checked_finite_add(exploitation, exploration_bonus, "fast LinUCB total score")?;
        Ok(LinUcbArmScore {
            arm,
            exploitation,
            exploration_bonus,
            total_score,
        })
    }
}

impl ContextualBandit for LinUcbFast {
    fn arm_count(&self) -> usize {
        self.arm_count
    }

    fn feature_count(&self) -> usize {
        self.feature_count
    }

    fn samples_seen(&self) -> u64 {
        self.samples_seen
    }

    fn select(&self, context: &[f64], rng: &mut impl Rng) -> Result<usize, RillError> {
        self.validate_context(context)?;
        let mut best_arm = 0;
        let mut best_score = f64::NEG_INFINITY;
        let mut tied = 0;
        for arm in 0..self.arm_count {
            let score = self.score_arm_validated(arm, context)?.total_score;
            if score > best_score {
                best_score = score;
                best_arm = arm;
                tied = 1;
            } else if score == best_score {
                tied += 1;
                if rng.gen_range(0..tied) == 0 {
                    best_arm = arm;
                }
            }
        }
        Ok(best_arm)
    }

    fn update(&mut self, arm: usize, context: &[f64], reward: f64) -> Result<(), RillError> {
        validate_arm(self.arm_count, arm)?;
        self.validate_context(context)?;
        validate_reward_finite(reward)?;
        let inverse = &self.inverse_matrices[arm];
        let projected = checked_matrix_vector_mul(inverse, context)?;
        let variance = checked_dot(context, &projected, "fast LinUCB update variance")?;
        let variance_tolerance = 1e-12
            * context
                .iter()
                .map(|value| value.abs())
                .sum::<f64>()
                .max(1.0);
        if variance < -variance_tolerance {
            return Err(RillError::InvalidState(
                "fast LinUCB inverse produced a negative update variance".to_owned(),
            ));
        }
        let denominator = checked_finite_add(1.0, variance, "Sherman-Morrison denominator")?;
        if denominator <= f64::EPSILON {
            return Err(RillError::InvalidState(
                "fast LinUCB Sherman-Morrison denominator is not positive".to_owned(),
            ));
        }

        let mut next_inverse = inverse.clone();
        for i in 0..self.feature_count {
            for j in 0..self.feature_count {
                let adjustment = projected[i] * projected[j] / denominator;
                next_inverse[i][j] = checked_finite_add(
                    next_inverse[i][j],
                    -adjustment,
                    "fast LinUCB inverse update",
                )?;
            }
        }
        // Explicitly mirror the lower triangle to remove round-off asymmetry.
        for i in 0..self.feature_count {
            let (previous_rows, current_and_later) = next_inverse.split_at_mut(i);
            let current_row = &mut current_and_later[0];
            for (j, previous_row) in previous_rows.iter().enumerate() {
                current_row[j] = previous_row[i];
            }
            if current_row[i] <= 0.0 {
                return Err(RillError::InvalidState(
                    "fast LinUCB inverse lost a positive diagonal".to_owned(),
                ));
            }
        }

        let mut next_b = self.b_vectors[arm].clone();
        for i in 0..self.feature_count {
            next_b[i] = checked_finite_add(next_b[i], reward * context[i], "fast LinUCB b vector")?;
        }
        let next_samples = checked_increment(self.samples_seen, "fast LinUCB samples_seen")?;
        self.inverse_matrices[arm] = next_inverse;
        self.b_vectors[arm] = next_b;
        self.samples_seen = next_samples;
        Ok(())
    }

    fn reset(&mut self) {
        for matrix in &mut self.inverse_matrices {
            *matrix = identity_matrix(self.feature_count);
        }
        for vector in &mut self.b_vectors {
            vector.fill(0.0);
        }
        self.samples_seen = 0;
    }
}

impl ValidateState for LinUcbFast {
    fn validate_state(&self) -> Result<(), RillError> {
        self.validate()
    }
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
struct LinUcbFastState {
    arm_count: usize,
    feature_count: usize,
    alpha: f64,
    inverse_matrices: Vec<Vec<Vec<f64>>>,
    b_vectors: Vec<Vec<f64>>,
    samples_seen: u64,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for LinUcbFast {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let state = LinUcbFastState::deserialize(deserializer)?;
        let bandit = Self {
            arm_count: state.arm_count,
            feature_count: state.feature_count,
            alpha: state.alpha,
            inverse_matrices: state.inverse_matrices,
            b_vectors: state.b_vectors,
            samples_seen: state.samples_seen,
        };
        bandit.validate().map_err(serde::de::Error::custom)?;
        Ok(bandit)
    }
}

#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
struct LinUcbState {
    arm_count: usize,
    feature_count: usize,
    alpha: f64,
    a_matrices: Vec<Vec<Vec<f64>>>,
    b_vectors: Vec<Vec<f64>>,
    samples_seen: u64,
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for LinUcb {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let state = LinUcbState::deserialize(deserializer)?;
        let bandit = Self {
            arm_count: state.arm_count,
            feature_count: state.feature_count,
            alpha: state.alpha,
            a_matrices: state.a_matrices,
            b_vectors: state.b_vectors,
            samples_seen: state.samples_seen,
        };
        bandit.validate().map_err(serde::de::Error::custom)?;
        Ok(bandit)
    }
}

#[cfg(feature = "serde")]
impl ValidateState for LinUcb {
    fn validate_state(&self) -> Result<(), RillError> {
        LinUcb::validate(self)
    }
}

// ---------------------------------------------------------------------------
// Matrix helpers (private)
// ---------------------------------------------------------------------------

/// Create a `d x d` identity matrix.
fn identity_matrix(d: usize) -> Vec<Vec<f64>> {
    let mut m = vec![vec![0.0; d]; d];
    for (i, row) in m.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    m
}

/// Check positive definiteness via a Cholesky decomposition.
fn matrix_is_positive_definite(matrix: &[Vec<f64>]) -> bool {
    cholesky_factor(matrix).is_ok()
}

/// Compute the lower-triangular Cholesky factor `L` where `A = L L^T`.
fn cholesky_factor(matrix: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, RillError> {
    let n = matrix.len();
    if n == 0 || matrix.iter().any(|row| row.len() != n) {
        return Err(RillError::InvalidState(
            "LinUCB matrix must be non-empty and square".to_owned(),
        ));
    }
    let mut lower = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let mut correction = 0.0;
            for (&left, &right) in lower[i][..j].iter().zip(&lower[j][..j]) {
                correction =
                    checked_finite_add(correction, left * right, "LinUCB Cholesky correction")?;
            }
            let residual = matrix[i][j] - correction;
            if i == j {
                if !residual.is_finite() || residual <= 0.0 {
                    return Err(RillError::InvalidState(
                        "LinUCB matrix is not positive definite".to_owned(),
                    ));
                }
                lower[i][j] = residual.sqrt();
            } else {
                lower[i][j] = residual / lower[j][j];
                if !lower[i][j].is_finite() {
                    return Err(RillError::InvalidState(
                        "LinUCB Cholesky factor is non-finite".to_owned(),
                    ));
                }
            }
        }
    }
    Ok(lower)
}

/// Solve `L L^T x = rhs` for a Cholesky factor `L`.
fn cholesky_solve(lower: &[Vec<f64>], rhs: &[f64]) -> Result<Vec<f64>, RillError> {
    let n = lower.len();
    if rhs.len() != n {
        return Err(RillError::DimensionMismatch {
            expected: n,
            actual: rhs.len(),
        });
    }
    let mut intermediate = vec![0.0; n];
    for i in 0..n {
        let mut correction = 0.0;
        for (j, value) in intermediate.iter().enumerate().take(i) {
            correction =
                checked_finite_add(correction, lower[i][j] * value, "LinUCB forward solve")?;
        }
        let value = (rhs[i] - correction) / lower[i][i];
        if !value.is_finite() {
            return Err(RillError::NonFiniteValue {
                field: "LinUCB forward solve",
                value,
            });
        }
        intermediate[i] = value;
    }

    let mut solution = vec![0.0; n];
    for i in (0..n).rev() {
        let mut correction = 0.0;
        for j in (i + 1)..n {
            correction = checked_finite_add(
                correction,
                lower[j][i] * solution[j],
                "LinUCB backward solve",
            )?;
        }
        let value = (intermediate[i] - correction) / lower[i][i];
        if !value.is_finite() {
            return Err(RillError::NonFiniteValue {
                field: "LinUCB backward solve",
                value,
            });
        }
        solution[i] = value;
    }
    Ok(solution)
}

fn inverse_from_cholesky(lower: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, RillError> {
    let n = lower.len();
    let mut inverse = vec![vec![0.0; n]; n];
    for column in 0..n {
        let mut basis = vec![0.0; n];
        basis[column] = 1.0;
        let solution = cholesky_solve(lower, &basis)?;
        for row in 0..n {
            inverse[row][column] = solution[row];
        }
    }
    // The mathematical inverse is symmetric. Mirroring removes harmless
    // solve-order round-off before it enters the fast-state validator.
    for i in 0..n {
        let (previous_rows, current_and_later) = inverse.split_at_mut(i);
        let current_row = &mut current_and_later[0];
        for (j, previous_row) in previous_rows.iter_mut().enumerate() {
            let symmetric = (current_row[j] + previous_row[i]) / 2.0;
            ensure_finite("LinUCB inverse", symmetric)?;
            current_row[j] = symmetric;
            previous_row[i] = symmetric;
        }
    }
    Ok(inverse)
}

fn checked_matrix_vector_mul(matrix: &[Vec<f64>], vector: &[f64]) -> Result<Vec<f64>, RillError> {
    let mut result = Vec::with_capacity(matrix.len());
    for row in matrix {
        result.push(checked_dot(row, vector, "LinUCB matrix-vector product")?);
    }
    Ok(result)
}

fn checked_dot(a: &[f64], b: &[f64], field: &'static str) -> Result<f64, RillError> {
    let mut result = 0.0;
    for (&left, &right) in a.iter().zip(b) {
        result = checked_finite_add(result, left * right, field)?;
    }
    Ok(result)
}

fn select_random_tie(scores: &[LinUcbArmScore], rng: &mut impl Rng) -> usize {
    let mut best_arm = scores[0].arm;
    let mut best_score = scores[0].total_score;
    let mut tied = 1usize;
    for score in &scores[1..] {
        if score.total_score > best_score {
            best_score = score.total_score;
            best_arm = score.arm;
            tied = 1;
        } else if score.total_score == best_score {
            tied += 1;
            if rng.gen_range(0..tied) == 0 {
                best_arm = score.arm;
            }
        }
    }
    best_arm
}

/// Compute the inverse of a square matrix via Gauss-Jordan elimination with
/// partial pivoting.
///
/// Returns an error if the matrix is singular (a zero pivot is encountered
/// after pivoting).
#[allow(clippy::needless_range_loop)]
#[cfg(test)]
fn matrix_inverse(matrix: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, RillError> {
    let n = matrix.len();
    // Build the augmented matrix [A | I].
    let mut aug = vec![vec![0.0; 2 * n]; n];
    for i in 0..n {
        for j in 0..n {
            aug[i][j] = matrix[i][j];
        }
        aug[i][n + i] = 1.0;
    }

    // Forward elimination with partial pivoting.
    for col in 0..n {
        // Find the pivot row with the largest absolute value in this column.
        let mut pivot = col;
        let mut max_val = aug[col][col].abs();
        for row in (col + 1)..n {
            if aug[row][col].abs() > max_val {
                max_val = aug[row][col].abs();
                pivot = row;
            }
        }
        if max_val < 1e-12 {
            return Err(RillError::InvalidParameter {
                name: "matrix",
                value: 0.0,
            });
        }
        if pivot != col {
            aug.swap(col, pivot);
        }
        // Scale the pivot row so the pivot element becomes 1.
        let pivot_val = aug[col][col];
        for j in 0..(2 * n) {
            aug[col][j] /= pivot_val;
        }
        // Eliminate all other rows.
        for row in 0..n {
            if row == col {
                continue;
            }
            let factor = aug[row][col];
            if factor == 0.0 {
                continue;
            }
            for j in 0..(2 * n) {
                aug[row][j] -= factor * aug[col][j];
            }
        }
    }

    // Extract the inverse from the right half of the augmented matrix.
    let mut inv = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..n {
            inv[i][j] = aug[i][n + j];
        }
    }
    Ok(inv)
}

/// Matrix-vector multiplication: `result = matrix * vector`.
#[cfg(test)]
fn matrix_vector_mul(matrix: &[Vec<f64>], vector: &[f64]) -> Vec<f64> {
    let n = matrix.len();
    let vector = &vector[..n];
    matrix
        .iter()
        .map(|row| {
            row[..n]
                .iter()
                .zip(vector)
                .map(|(matrix_value, vector_value)| matrix_value * vector_value)
                .sum()
        })
        .collect()
}

/// Dot product of two slices.
#[cfg(test)]
fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Quadratic form `x^T * matrix * x`.
#[cfg(test)]
fn quadratic_form(x: &[f64], matrix: &[Vec<f64>]) -> f64 {
    let n = x.len();
    let mut result = 0.0;
    for i in 0..n {
        for j in 0..n {
            result += x[i] * matrix[i][j] * x[j];
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    fn make_bandit() -> LinUcb {
        LinUcb::new(LinUcbConfig {
            alpha: 1.0,
            arm_count: 3,
            feature_count: 2,
        })
        .unwrap()
    }

    #[test]
    fn rejects_zero_arm_count() {
        let result = LinUcb::new(LinUcbConfig {
            alpha: 1.0,
            arm_count: 0,
            feature_count: 2,
        });
        assert!(matches!(result, Err(RillError::InvalidArmCount(0))));
    }

    #[test]
    fn rejects_zero_feature_count() {
        let result = LinUcb::new(LinUcbConfig {
            alpha: 1.0,
            arm_count: 3,
            feature_count: 0,
        });
        assert!(matches!(result, Err(RillError::InvalidFeatureCount(0))));
    }

    #[test]
    fn rejects_invalid_alpha() {
        for &bad in &[0.0, -1.0, f64::NAN, f64::INFINITY] {
            let result = LinUcb::new(LinUcbConfig {
                alpha: bad,
                arm_count: 3,
                feature_count: 2,
            });
            assert!(matches!(result, Err(RillError::InvalidParameter { .. })));
        }
    }

    #[test]
    fn initial_state() {
        let b = make_bandit();
        assert_eq!(b.arm_count(), 3);
        assert_eq!(b.feature_count(), 2);
        assert_eq!(b.samples_seen(), 0);
        assert!((b.alpha() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn initial_a_is_identity() {
        let b = make_bandit();
        let a = b.a_matrix(0).unwrap();
        assert!((a[0][0] - 1.0).abs() < 1e-12);
        assert!((a[0][1] - 0.0).abs() < 1e-12);
        assert!((a[1][0] - 0.0).abs() < 1e-12);
        assert!((a[1][1] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn initial_b_is_zero() {
        let b = make_bandit();
        let bv = b.b_vector(0).unwrap();
        assert!((bv[0] - 0.0).abs() < 1e-12);
        assert!((bv[1] - 0.0).abs() < 1e-12);
    }

    #[test]
    fn initial_ties_are_randomized() {
        let b = make_bandit();
        let mut rng = ChaCha8Rng::seed_from_u64(12);
        let mut seen = std::collections::HashSet::new();
        for _ in 0..100 {
            seen.insert(b.select(&[0.5, 0.8], &mut rng).unwrap());
        }
        assert_eq!(seen.len(), b.arm_count());
    }

    #[test]
    fn score_breakdown_is_finite_and_sums_to_total() {
        let b = make_bandit();
        let scores = b.score_all(&[0.5, 0.8]).unwrap();
        assert_eq!(scores.len(), 3);
        for (arm, score) in scores.iter().enumerate() {
            assert_eq!(score.arm, arm);
            assert!(score.exploitation.is_finite());
            assert!(score.exploration_bonus.is_finite());
            assert!(score.total_score.is_finite());
            assert_eq!(
                score.total_score,
                score.exploitation + score.exploration_bonus
            );
            assert_eq!(score.exploitation, 0.0);
            assert!((score.exploration_bonus - (0.5_f64 * 0.5 + 0.8 * 0.8).sqrt()).abs() < 1e-12);
        }
    }

    #[test]
    fn score_arm_validates_arm_context_and_dimension() {
        let b = make_bandit();
        assert!(matches!(
            b.score_arm(3, &[0.5, 0.8]),
            Err(RillError::InvalidArm { .. })
        ));
        assert!(matches!(
            b.score_arm(0, &[0.5]),
            Err(RillError::DimensionMismatch { .. })
        ));
        assert!(matches!(
            b.score_arm(0, &[f64::NAN, 0.8]),
            Err(RillError::NonFiniteValue { .. })
        ));
    }

    #[test]
    fn select_with_scores_matches_select_including_random_ties() {
        let b = make_bandit();
        for seed in 0..100 {
            let mut select_rng = ChaCha8Rng::seed_from_u64(seed);
            let mut scores_rng = ChaCha8Rng::seed_from_u64(seed);
            let selected = b.select(&[0.5, 0.8], &mut select_rng).unwrap();
            let (explained, scores) = b.select_with_scores(&[0.5, 0.8], &mut scores_rng).unwrap();
            assert_eq!(explained, selected);
            assert_eq!(scores.len(), 3);
        }
    }

    #[test]
    fn deterministic_selection_uses_lowest_index_for_exact_ties() {
        let b = make_bandit();
        assert_eq!(b.select_deterministic(&[0.5, 0.8]).unwrap(), 0);
    }

    #[test]
    fn single_arm_scoring_and_selection() {
        let b = LinUcb::new(LinUcbConfig {
            alpha: 0.5,
            arm_count: 1,
            feature_count: 2,
        })
        .unwrap();
        let score = b.score_arm(0, &[3.0, 4.0]).unwrap();
        assert_eq!(score.arm, 0);
        assert_eq!(score.exploitation, 0.0);
        assert!((score.exploration_bonus - 2.5).abs() < 1e-12);
        assert_eq!(b.select_deterministic(&[3.0, 4.0]).unwrap(), 0);
    }

    #[test]
    fn select_returns_valid_arm() {
        let b = make_bandit();
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let context = [0.5, 0.8];
        let arm = b.select(&context, &mut rng).unwrap();
        assert!(arm < 3);
    }

    #[test]
    fn update_modifies_a_and_b() {
        let mut b = make_bandit();
        let context = [0.5, 0.8];
        b.update(0, &context, 1.0).unwrap();

        let a = b.a_matrix(0).unwrap();
        // A = I + x * x^T
        assert!((a[0][0] - (1.0 + 0.5 * 0.5)).abs() < 1e-12);
        assert!((a[0][1] - (0.5 * 0.8)).abs() < 1e-12);
        assert!((a[1][0] - (0.8 * 0.5)).abs() < 1e-12);
        assert!((a[1][1] - (1.0 + 0.8 * 0.8)).abs() < 1e-12);

        let bv = b.b_vector(0).unwrap();
        assert!((bv[0] - 0.5).abs() < 1e-12);
        assert!((bv[1] - 0.8).abs() < 1e-12);
        assert_eq!(b.samples_seen(), 1);
    }

    #[test]
    fn update_does_not_affect_other_arms() {
        let mut b = make_bandit();
        let context = [0.5, 0.8];
        b.update(0, &context, 1.0).unwrap();

        // Arm 1 should still be at the initial state.
        let a1 = b.a_matrix(1).unwrap();
        assert!((a1[0][0] - 1.0).abs() < 1e-12);
        let b1 = b.b_vector(1).unwrap();
        assert!((b1[0] - 0.0).abs() < 1e-12);
    }

    #[test]
    fn select_rejects_wrong_context_length() {
        let b = make_bandit();
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        assert!(b.select(&[0.5], &mut rng).is_err());
        assert!(b.select(&[0.5, 0.8, 0.9], &mut rng).is_err());
    }

    #[test]
    fn select_rejects_non_finite_context() {
        let b = make_bandit();
        let mut rng = ChaCha8Rng::seed_from_u64(0);
        assert!(b.select(&[f64::NAN, 0.8], &mut rng).is_err());
        assert!(b.select(&[0.5, f64::INFINITY], &mut rng).is_err());
    }

    #[test]
    fn update_rejects_invalid_arm() {
        let mut b = make_bandit();
        let context = [0.5, 0.8];
        assert!(b.update(3, &context, 1.0).is_err());
    }

    #[test]
    fn update_rejects_wrong_context_length() {
        let mut b = make_bandit();
        assert!(b.update(0, &[0.5], 1.0).is_err());
        assert!(b.update(0, &[0.5, 0.8, 0.9], 1.0).is_err());
    }

    #[test]
    fn update_rejects_non_finite_reward() {
        let mut b = make_bandit();
        let context = [0.5, 0.8];
        assert!(b.update(0, &context, f64::NAN).is_err());
        assert!(b.update(0, &context, f64::INFINITY).is_err());
    }

    #[test]
    fn update_rejects_arithmetic_overflow_without_mutating_state() {
        let mut b = make_bandit();
        let before = b.clone();
        assert!(b.update(0, &[f64::MAX, f64::MAX], 1.0).is_err());
        assert_eq!(b.a_matrices, before.a_matrices);
        assert_eq!(b.b_vectors, before.b_vectors);
        assert_eq!(b.samples_seen(), before.samples_seen());
    }

    #[test]
    fn update_rejects_numerically_degenerate_matrix_without_mutating_state() {
        let mut b = make_bandit();
        let before = b.clone();
        let result = b.update(0, &[1e150, 1e150], 1.0);
        assert!(matches!(result, Err(RillError::InvalidState(_))));
        assert_eq!(b.a_matrices, before.a_matrices);
        assert_eq!(b.b_vectors, before.b_vectors);
        assert_eq!(b.samples_seen(), before.samples_seen());
    }

    #[test]
    fn scoring_is_side_effect_free() {
        let mut b = make_bandit();
        b.update(1, &[0.25, -0.75], 2.0).unwrap();
        let before = b.clone();
        let _ = b.score_all(&[0.75, 0.5]).unwrap();
        let _ = b.select_deterministic(&[0.75, 0.5]).unwrap();
        assert_eq!(b.a_matrices, before.a_matrices);
        assert_eq!(b.b_vectors, before.b_vectors);
        assert_eq!(b.samples_seen, before.samples_seen);
    }

    #[test]
    fn cholesky_scores_match_explicit_inverse_reference() {
        let mut b = make_bandit();
        for i in 1..=50 {
            let context = [i as f64 / 17.0, (i as f64).sin()];
            b.update(i % 3, &context, (i as f64 / 7.0).cos()).unwrap();
        }
        let context = [0.75, -1.25];
        for arm in 0..3 {
            let inverse = matrix_inverse(&b.a_matrices[arm]).unwrap();
            let theta = matrix_vector_mul(&inverse, &b.b_vectors[arm]);
            let expected_exploitation = dot(&theta, &context);
            let expected_bonus = b.alpha * quadratic_form(&context, &inverse).sqrt();
            let score = b.score_arm(arm, &context).unwrap();
            assert!((score.exploitation - expected_exploitation).abs() < 1e-10);
            assert!((score.exploration_bonus - expected_bonus).abs() < 1e-10);
        }
    }

    #[test]
    fn condition_diagnostics_are_finite() {
        let mut b = make_bandit();
        for _ in 0..1000 {
            b.update(0, &[1e-6, 1e3], 0.25).unwrap();
        }
        let diagnostics = b.condition_diagnostics(0).unwrap();
        assert_eq!(diagnostics.arm, 0);
        assert!(diagnostics.min_cholesky_diagonal > 0.0);
        assert!(diagnostics.max_cholesky_diagonal.is_finite());
        assert!(diagnostics.condition_indicator.is_finite());
        assert!(diagnostics.condition_indicator >= 1.0);
    }

    #[test]
    fn a_matrix_rejects_invalid_arm() {
        let b = make_bandit();
        assert!(b.a_matrix(5).is_err());
    }

    #[test]
    fn b_vector_rejects_invalid_arm() {
        let b = make_bandit();
        assert!(b.b_vector(5).is_err());
    }

    #[test]
    fn reset_clears_state() {
        let mut b = make_bandit();
        let context = [0.5, 0.8];
        b.update(0, &context, 1.0).unwrap();
        b.update(1, &context, 0.5).unwrap();
        assert_eq!(b.samples_seen(), 2);

        b.reset();
        assert_eq!(b.samples_seen(), 0);
        // A should be back to identity.
        let a = b.a_matrix(0).unwrap();
        assert!((a[0][0] - 1.0).abs() < 1e-12);
        // b should be back to zero.
        let bv = b.b_vector(0).unwrap();
        assert!((bv[0] - 0.0).abs() < 1e-12);
    }

    #[test]
    fn identity_matrix_inverse_is_identity() {
        let ident = identity_matrix(3);
        let inv = matrix_inverse(&ident).unwrap();
        for (i, row) in inv.iter().enumerate() {
            for (j, &val) in row.iter().enumerate() {
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!((val - expected).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn known_2x2_matrix_inverse() {
        // [[4, 7], [2, 6]] inverse = [[0.6, -0.7], [-0.2, 0.4]]
        let matrix = vec![vec![4.0, 7.0], vec![2.0, 6.0]];
        let inv = matrix_inverse(&matrix).unwrap();
        assert!((inv[0][0] - 0.6).abs() < 1e-10);
        assert!((inv[0][1] - (-0.7)).abs() < 1e-10);
        assert!((inv[1][0] - (-0.2)).abs() < 1e-10);
        assert!((inv[1][1] - 0.4).abs() < 1e-10);
    }

    #[test]
    fn singular_matrix_inverse_returns_error() {
        // A singular matrix (second row is a multiple of the first).
        let matrix = vec![vec![1.0, 2.0], vec![2.0, 4.0]];
        let result = matrix_inverse(&matrix);
        assert!(result.is_err());
    }

    #[test]
    fn dot_product_correct() {
        assert!((dot(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]) - 32.0).abs() < 1e-12);
    }

    #[test]
    fn matrix_vector_mul_correct() {
        let m = vec![vec![1.0, 2.0], vec![3.0, 4.0]];
        let v = vec![5.0, 6.0];
        let r = matrix_vector_mul(&m, &v);
        assert!((r[0] - 17.0).abs() < 1e-12);
        assert!((r[1] - 39.0).abs() < 1e-12);
    }

    #[test]
    fn quadratic_form_correct() {
        // For identity matrix, x^T * I * x = sum(x_i^2)
        let ident = identity_matrix(3);
        let x = [1.0, 2.0, 3.0];
        let q = quadratic_form(&x, &ident);
        assert!((q - 14.0).abs() < 1e-12);
    }

    #[test]
    fn contextual_selection_prefers_aligned_arm() {
        // Two arms, 2-d context. Train arm 0 with context [1, 0] and reward 1,
        // arm 1 with context [0, 1] and reward 1. When asked to select with
        // context [1, 0], arm 0 should be preferred (its model aligns with
        // this context).
        let mut b = LinUcb::new(LinUcbConfig {
            alpha: 0.1,
            arm_count: 2,
            feature_count: 2,
        })
        .unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(7);

        // Train arm 0 with context [1, 0] and high reward.
        for _ in 0..20 {
            b.update(0, &[1.0, 0.0], 1.0).unwrap();
        }
        // Train arm 1 with context [0, 1] and high reward.
        for _ in 0..20 {
            b.update(1, &[0.0, 1.0], 1.0).unwrap();
        }

        // Query with context [1, 0]: arm 0 should be selected.
        let arm = b.select(&[1.0, 0.0], &mut rng).unwrap();
        assert_eq!(arm, 0);

        // Query with context [0, 1]: arm 1 should be selected.
        let arm = b.select(&[0.0, 1.0], &mut rng).unwrap();
        assert_eq!(arm, 1);
    }

    #[test]
    fn learns_to_prefer_high_reward_arm() {
        // 2 arms, 1-d context. Arm 0 gives reward proportional to context,
        // arm 1 gives low reward. LinUCB should learn to prefer arm 0.
        let mut b = LinUcb::new(LinUcbConfig {
            alpha: 0.5,
            arm_count: 2,
            feature_count: 1,
        })
        .unwrap();
        let mut rng = ChaCha8Rng::seed_from_u64(99);

        for step in 1..=100 {
            let x = step as f64 * 0.1;
            let arm = b.select(&[x], &mut rng).unwrap();
            // Arm 0: reward = 2*x; arm 1: reward = 0.1*x.
            let reward = if arm == 0 { 2.0 * x } else { 0.1 * x };
            b.update(arm, &[x], reward).unwrap();
        }

        // After learning, arm 0 should be selected for a typical context.
        let final_arm = b.select(&[5.0], &mut rng).unwrap();
        assert_eq!(final_arm, 0);
    }

    #[test]
    fn fast_linucb_matches_stable_scores_and_selection() {
        let config = LinUcbConfig {
            alpha: 0.35,
            arm_count: 4,
            feature_count: 6,
        };
        let mut stable = LinUcb::new(config.clone()).unwrap();
        let mut fast = LinUcbFast::new(config).unwrap();
        for step in 0..2000 {
            let context: Vec<f64> = (0..6)
                .map(|feature| ((step + feature * 17) as f64 / 23.0).sin())
                .collect();
            let arm = step % 4;
            let reward = (arm as f64 + 1.0) * context[arm % 6] + 0.1;
            stable.update(arm, &context, reward).unwrap();
            fast.update(arm, &context, reward).unwrap();
        }
        let context = [0.75, -0.25, 1.0, 0.1, -0.5, 0.9];
        let stable_scores = stable.score_all(&context).unwrap();
        let fast_scores = fast.score_all(&context).unwrap();
        for (stable_score, fast_score) in stable_scores.iter().zip(&fast_scores) {
            assert_eq!(stable_score.arm, fast_score.arm);
            assert!((stable_score.exploitation - fast_score.exploitation).abs() < 1e-8);
            assert!((stable_score.exploration_bonus - fast_score.exploration_bonus).abs() < 1e-8);
            assert!((stable_score.total_score - fast_score.total_score).abs() < 1e-8);
        }
        assert_eq!(
            stable.select_deterministic(&context).unwrap(),
            fast.select_deterministic(&context).unwrap()
        );
        assert_eq!(stable.samples_seen(), fast.samples_seen());
        fast.validate().unwrap();
    }

    #[test]
    fn fast_conversion_matches_stable_state() {
        let mut stable = make_bandit();
        for step in 0..100 {
            let context = [(step as f64 / 11.0).sin(), (step as f64 / 7.0).cos()];
            stable
                .update(step % 3, &context, step as f64 / 100.0)
                .unwrap();
        }
        let fast = LinUcbFast::from_linucb(&stable).unwrap();
        let context = [0.25, -0.75];
        for arm in 0..3 {
            let stable_score = stable.score_arm(arm, &context).unwrap();
            let fast_score = fast.score_arm(arm, &context).unwrap();
            assert!((stable_score.total_score - fast_score.total_score).abs() < 1e-10);
        }
        assert_eq!(fast.state_f64_count(), 3 * (2 * 2 + 2) + 1);
    }

    #[test]
    fn fast_update_failure_is_atomic() {
        let mut fast = LinUcbFast::new(LinUcbConfig {
            alpha: 1.0,
            arm_count: 2,
            feature_count: 2,
        })
        .unwrap();
        let before = fast.clone();
        assert!(fast.update(0, &[1e200, 1e200], 1.0).is_err());
        assert_eq!(fast.inverse_matrices, before.inverse_matrices);
        assert_eq!(fast.b_vectors, before.b_vectors);
        assert_eq!(fast.samples_seen, before.samples_seen);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn fast_serde_roundtrip_preserves_future_continuity() {
        let mut original = LinUcbFast::new(LinUcbConfig {
            alpha: 0.5,
            arm_count: 2,
            feature_count: 3,
        })
        .unwrap();
        original.update(0, &[1.0, 0.5, -0.5], 1.0).unwrap();
        let json = serde_json::to_string(&original).unwrap();
        let mut restored: LinUcbFast = serde_json::from_str(&json).unwrap();
        for step in 0..100 {
            let context = [step as f64 / 100.0, 0.25, -0.75];
            original.update(step % 2, &context, 0.5).unwrap();
            restored.update(step % 2, &context, 0.5).unwrap();
            assert_eq!(
                original.score_all(&context).unwrap(),
                restored.score_all(&context).unwrap()
            );
        }
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_roundtrip() {
        let mut b = LinUcb::new(LinUcbConfig {
            alpha: 1.5,
            arm_count: 2,
            feature_count: 3,
        })
        .unwrap();
        b.update(0, &[1.0, 0.5, 0.2], 1.0).unwrap();
        b.update(1, &[0.3, 0.7, 0.9], 0.5).unwrap();

        let json = serde_json::to_string(&b).unwrap();
        let restored: LinUcb = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.arm_count(), b.arm_count());
        assert_eq!(restored.feature_count(), b.feature_count());
        assert_eq!(restored.samples_seen(), b.samples_seen());
        assert!((restored.alpha() - b.alpha()).abs() < 1e-12);
        // Verify A matrix is preserved.
        let orig_a = b.a_matrix(0).unwrap();
        let rest_a = restored.a_matrix(0).unwrap();
        for (orig_row, rest_row) in orig_a.iter().zip(rest_a.iter()) {
            for (&o, &r) in orig_row.iter().zip(rest_row.iter()) {
                assert!((o - r).abs() < 1e-12);
            }
        }
        assert_eq!(
            restored.score_all(&[0.2, -0.5, 1.0]).unwrap(),
            b.score_all(&[0.2, -0.5, 1.0]).unwrap()
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_rejects_malformed_state() {
        let json = r#"{
            "arm_count": 2,
            "feature_count": 2,
            "alpha": 1.0,
            "a_matrices": [[[1.0, 0.0], [0.0, 1.0]]],
            "b_vectors": [[0.0, 0.0]],
            "samples_seen": 0
        }"#;
        assert!(serde_json::from_str::<LinUcb>(json).is_err());
    }
}
