//! Integration tests: online statistics vs. batch reference formulas.
//!
//! These tests verify that the incremental algorithms (Welford, EWMean, etc.)
//! produce values matching closed-form batch computations on the same data.

use rill_ml::OnlineStatistic;
use rill_ml::stats::{
    Count, ExponentiallyWeightedMean, Max, Mean, Min, RollingMean, RollingVariance, Sum, Variance,
    VarianceKind,
};

fn batch_mean(data: &[f64]) -> f64 {
    data.iter().sum::<f64>() / data.len() as f64
}

fn batch_population_variance(data: &[f64]) -> f64 {
    let mean = batch_mean(data);
    data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / data.len() as f64
}

fn batch_sample_variance(data: &[f64]) -> f64 {
    let mean = batch_mean(data);
    data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (data.len() - 1) as f64
}

#[test]
fn mean_matches_batch_formula() {
    let data: Vec<f64> = (0..1000).map(|i| i as f64 * 0.1 - 50.0).collect();
    let mut m = Mean::new();
    for &x in &data {
        m.update(x).unwrap();
    }
    let expected = batch_mean(&data);
    assert!(
        (m.value() - expected).abs() < 1e-9,
        "mean = {}, expected = {}",
        m.value(),
        expected
    );
    assert_eq!(m.samples_seen(), 1000);
}

#[test]
fn population_variance_matches_batch_formula() {
    let data: Vec<f64> = (0..500).map(|i| (i as f64 * 0.7).sin()).collect();
    let mut v = Variance::new(VarianceKind::Population);
    for &x in &data {
        v.update(x).unwrap();
    }
    let expected = batch_population_variance(&data);
    assert!(
        (v.value().unwrap() - expected).abs() < 1e-9,
        "variance = {}, expected = {}",
        v.value().unwrap(),
        expected
    );
}

#[test]
fn sample_variance_matches_batch_formula() {
    let data: Vec<f64> = (0..500).map(|i| (i as f64 * 0.3).cos()).collect();
    let mut v = Variance::new(VarianceKind::Sample);
    for &x in &data {
        v.update(x).unwrap();
    }
    let expected = batch_sample_variance(&data);
    assert!(
        (v.value().unwrap() - expected).abs() < 1e-9,
        "sample variance = {}, expected = {}",
        v.value().unwrap(),
        expected
    );
}

#[test]
fn ew_mean_matches_manual_recursion() {
    let alpha = 0.25;
    let data = [1.0, 2.5, 0.3, 4.1, 2.2, 3.8, 1.5, 0.9, 5.0, 2.7];
    let mut ew = ExponentiallyWeightedMean::new(alpha).unwrap();
    let mut manual = None;
    for &x in &data {
        ew.update(x).unwrap();
        manual = match manual {
            None => Some(x),
            Some(prev) => Some(alpha * x + (1.0 - alpha) * prev),
        };
    }
    let online_val = ew.value();
    let manual_val = manual.unwrap();
    assert!(
        (online_val - manual_val).abs() < 1e-12,
        "ew_mean = {}, manual = {}",
        online_val,
        manual_val
    );
}

#[test]
fn rolling_mean_matches_batch_window() {
    let window = 10;
    let data: Vec<f64> = (0..100).map(|i| i as f64).collect();
    let mut rm = RollingMean::new(window).unwrap();
    for (i, &x) in data.iter().enumerate() {
        rm.update(x).unwrap();
        let start = if i >= window { i + 1 - window } else { 0 };
        let expected = batch_mean(&data[start..=i]);
        assert!(
            (rm.value().unwrap() - expected).abs() < 1e-9,
            "at i={i}: rolling_mean = {}, expected = {}",
            rm.value().unwrap(),
            expected
        );
    }
}

#[test]
fn rolling_variance_matches_batch_window() {
    let window = 8;
    let data: Vec<f64> = (0..50).map(|i| (i as f64 * 0.5).sin()).collect();
    let mut rv = RollingVariance::new(window, VarianceKind::Population).unwrap();
    for (i, &x) in data.iter().enumerate() {
        rv.update(x).unwrap();
        let start = if i >= window { i + 1 - window } else { 0 };
        let expected = batch_population_variance(&data[start..=i]);
        let online = rv.value().unwrap();
        assert!(
            (online - expected).abs() < 1e-9,
            "at i={i}: rolling_var = {}, expected = {}",
            online,
            expected
        );
    }
}

#[test]
fn count_and_sum_match_reference() {
    let data: Vec<f64> = (0..200).map(|i| i as f64).collect();
    let mut count = Count::new();
    let mut sum = Sum::new();
    for &x in &data {
        count.update(x).unwrap();
        sum.update(x).unwrap();
    }
    assert_eq!(count.value(), 200);
    assert_eq!(sum.value(), data.iter().sum::<f64>());
}

#[test]
fn min_max_match_reference() {
    let data: Vec<f64> = (0..100)
        .map(|i| ((i as f64 * 0.3).sin() * 10.0).round())
        .collect();
    let mut min = Min::new();
    let mut max = Max::new();
    for &x in &data {
        min.update(x).unwrap();
        max.update(x).unwrap();
    }
    assert_eq!(
        min.value(),
        Some(data.iter().copied().fold(f64::INFINITY, f64::min))
    );
    assert_eq!(
        max.value(),
        Some(data.iter().copied().fold(f64::NEG_INFINITY, f64::max))
    );
}

#[test]
fn reset_clears_all_state() {
    let mut m = Mean::new();
    let mut v = Variance::new(VarianceKind::Population);
    for i in 0..50 {
        m.update(i as f64).unwrap();
        v.update(i as f64).unwrap();
    }
    m.reset();
    v.reset();
    assert_eq!(m.samples_seen(), 0);
    assert_eq!(v.samples_seen(), 0);
    assert!(v.value().is_none());
}
