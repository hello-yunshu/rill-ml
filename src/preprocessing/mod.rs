//! Feature preprocessing transformers.
//!
//! All transformers maintain per-feature online statistics and use `O(d)`
//! memory where `d` is the feature dimension.

pub mod clipper;
pub mod constant_imputer;
pub mod forward_fill;
pub mod frequency;
pub mod mean_imputer;
pub mod min_max_scaler;
pub mod missing_indicator;
pub mod one_hot;
pub mod ordinal;
pub mod standard_scaler;

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
