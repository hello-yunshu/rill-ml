//! Feature preprocessing transformers.
//!
//! All transformers maintain per-feature online statistics and use `O(d)`
//! memory where `d` is the feature dimension.

pub(crate) mod clipper;
pub(crate) mod constant_imputer;
pub(crate) mod forward_fill;
pub(crate) mod frequency;
pub(crate) mod mean_imputer;
pub(crate) mod min_max_scaler;
pub(crate) mod missing_indicator;
pub(crate) mod one_hot;
pub(crate) mod ordinal;
pub(crate) mod standard_scaler;

pub use clipper::Clipper;
pub use constant_imputer::{ConstantImputer, ConstantImputerConfig};
pub use forward_fill::ForwardFill;
pub use frequency::FrequencyEncoder;
pub use mean_imputer::MeanImputer;
pub use min_max_scaler::MinMaxScaler;
pub use missing_indicator::MissingIndicator;
pub use one_hot::OneHotEncoder;
pub use ordinal::OrdinalEncoder;
pub use standard_scaler::{StandardScaler, StandardScalerConfig};
