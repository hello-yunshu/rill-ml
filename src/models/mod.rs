//! Online models: baseline regressors, linear regression, logistic regression,
//! Naive Bayes classifiers, and FTRL-Proximal for sparse features.

pub(crate) mod baseline;
pub(crate) mod ftrl;
pub(crate) mod linear_regression;
pub(crate) mod logistic_regression;
pub(crate) mod naive_bayes;

pub use baseline::{
    BaselineConfig, ExponentiallyWeightedMeanRegressor, LastValueRegressor, MeanRegressor,
};
pub use ftrl::{
    FtrlClassifier, FtrlConfig, FtrlRegressor, FtrlResourceDiagnostics, NewFeaturePolicy,
};
pub use linear_regression::{LinearRegression, LinearRegressionConfig};
pub use logistic_regression::{LogisticRegression, LogisticRegressionConfig};
pub use naive_bayes::{
    BernoulliNaiveBayes, GaussianNaiveBayes, MultinomialNaiveBayes, NaiveBayesConfig,
};
