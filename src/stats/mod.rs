//! Online univariate statistics with bounded memory.
//!
//! Every non-rolling statistic in this module uses `O(1)` memory.
//! Rolling statistics use `O(window_size)` memory.

pub mod count;
pub mod ew_mean;
pub mod extrema;
pub mod mean;
pub mod rolling;
pub mod sum;
pub mod variance;

pub use count::Count;
pub use ew_mean::ExponentiallyWeightedMean;
pub use extrema::{Max, Min};
pub use mean::Mean;
pub use rolling::{RollingMean, RollingVariance};
pub use sum::Sum;
pub use variance::{Variance, VarianceKind};
