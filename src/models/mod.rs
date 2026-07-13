//! Online models: baseline regressors, linear regression, logistic regression,
//! Naive Bayes classifiers, and FTRL-Proximal for sparse features.

pub mod baseline;
pub mod ftrl;
pub mod linear_regression;
pub mod logistic_regression;
pub mod naive_bayes;

pub use baseline::{
    BaselineConfig, ExponentiallyWeightedMeanRegressor, LastValueRegressor, MeanRegressor,
};
pub use ftrl::{FtrlClassifier, FtrlConfig, FtrlRegressor};
pub use linear_regression::{LinearRegression, LinearRegressionConfig};
pub use logistic_regression::{LogisticRegression, LogisticRegressionConfig};
pub use naive_bayes::{
    BernoulliNaiveBayes, GaussianNaiveBayes, MultinomialNaiveBayes, NaiveBayesConfig,
};
