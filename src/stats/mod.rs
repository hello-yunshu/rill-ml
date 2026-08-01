//! Online univariate statistics with bounded memory.
//!
//! Every non-rolling statistic in this module uses `O(1)` memory.
//! Rolling statistics use `O(window_size)` memory.

pub(crate) mod count;
pub(crate) mod ew_mean;
pub(crate) mod extrema;
pub(crate) mod mean;
pub(crate) mod quantile;
pub(crate) mod robust;
pub(crate) mod rolling;
pub(crate) mod sum;
pub(crate) mod variance;

pub use count::Count;
pub use ew_mean::ExponentiallyWeightedMean;
pub use extrema::{Max, Min};
pub use mean::Mean;
pub use quantile::{P2Quantile, P2Quantiles};
pub use robust::{
    ClippedMean, MAD_NORMAL_CONSISTENCY_SCALE, MAX_ROBUST_WINDOW_SIZE, MODIFIED_Z_NORMAL_FACTOR,
    MedianMadSummary, ModifiedZScore, RollingMedianMad,
};
pub use rolling::{RollingMean, RollingVariance};
pub use sum::Sum;
pub use variance::{Variance, VarianceKind};
