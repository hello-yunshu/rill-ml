use chrono::{DateTime, Datelike, Local, Timelike, Utc};
use rill_ml::{
    OnlineRegressor,
    diagnostics::BaselineComparator,
    loss::{HuberLoss, RegressionLoss},
    models::{LinearRegression, LinearRegressionConfig},
    optim::{Optimizer, SgdConfig},
};
use rill_runtime_protocol::{
    BatteryModelConfig, BatteryPredictionInput, BatteryPredictionOutput, BatterySampleInput,
    PredictionSource,
};
use thiserror::Error;

const MAX_SAMPLES: usize = 10_000;

#[derive(Debug, Clone)]
struct DrainObservation {
    at: DateTime<Utc>,
    ended_at: DateTime<Utc>,
    percentage: u8,
    drain_per_hour: f64,
}

#[derive(Debug, Error)]
pub enum BatteryPredictionError {
    #[error("battery history exceeds {MAX_SAMPLES} samples")]
    TooManySamples,
    #[error("invalid prediction timestamp")]
    InvalidNow,
    #[error("invalid battery sample at index {index}")]
    InvalidSample { index: usize },
    #[error("unable to initialize the configured model")]
    InvalidModel,
}

pub fn predict(
    input: &BatteryPredictionInput,
    config: &BatteryModelConfig,
) -> Result<BatteryPredictionOutput, BatteryPredictionError> {
    if input.samples.len() > MAX_SAMPLES {
        return Err(BatteryPredictionError::TooManySamples);
    }
    let now = DateTime::from_timestamp_millis(input.now_unix_ms)
        .ok_or(BatteryPredictionError::InvalidNow)?;
    let mut samples = input.samples.clone();
    for (index, sample) in samples.iter().enumerate() {
        if sample.percentage > 100 || DateTime::from_timestamp_millis(sample.at_unix_ms).is_none() {
            return Err(BatteryPredictionError::InvalidSample { index });
        }
    }
    samples.retain(|sample| sample.at_unix_ms <= input.now_unix_ms);
    samples.sort_by_key(|sample| sample.at_unix_ms);

    let Some(current) = samples
        .iter()
        .filter(|sample| !sample.charging && sample.at_unix_ms <= input.now_unix_ms)
        .max_by_key(|sample| sample.at_unix_ms)
    else {
        return Ok(fallback("noDischargingSample", 0, 0, None, None));
    };
    if current.percentage == 0 {
        return Ok(fallback("emptyBattery", 0, 0, None, None));
    }

    let observations = discharge_observations(&samples, config);
    validated_model_prediction(&observations, current.percentage, now, config)
}

fn validated_model_prediction(
    observations: &[DrainObservation],
    current_percentage: u8,
    now: DateTime<Utc>,
    config: &BatteryModelConfig,
) -> Result<BatteryPredictionOutput, BatteryPredictionError> {
    let optimizer = Optimizer::sgd(
        config.feature_count,
        SgdConfig {
            learning_rate: config.learning_rate,
            l2: config.l2,
        },
    )
    .map_err(|_| BatteryPredictionError::InvalidModel)?;
    let mut model = LinearRegression::new(
        config.feature_count,
        LinearRegressionConfig {
            optimizer,
            loss: RegressionLoss::Huber(
                HuberLoss::new(config.huber_delta)
                    .map_err(|_| BatteryPredictionError::InvalidModel)?,
            ),
        },
    )
    .map_err(|_| BatteryPredictionError::InvalidModel)?;
    let mut comparator = BaselineComparator::new(
        &["deterministic-baseline", "rill-local-ai"],
        config.quality_window,
    )
    .map_err(|_| BatteryPredictionError::InvalidModel)?;

    for (index, observation) in observations.iter().enumerate() {
        let recent_rate = weighted_baseline_rate(
            &observations[..index],
            observation.at,
            config.baseline_decay_tau_hours,
        );
        let features = features(observation.percentage, observation.at, recent_rate);
        if model.samples_seen() >= config.min_training_samples {
            if let Some(baseline_prediction) = recent_rate {
                if let Ok(ai_prediction) = model.predict(&features) {
                    if ai_prediction.is_finite() {
                        comparator
                            .record(0, observation.drain_per_hour, baseline_prediction)
                            .map_err(|_| BatteryPredictionError::InvalidModel)?;
                        comparator
                            .record(1, observation.drain_per_hour, ai_prediction)
                            .map_err(|_| BatteryPredictionError::InvalidModel)?;
                    }
                }
            }
        }
        model
            .learn(&features, observation.drain_per_hour)
            .map_err(|_| BatteryPredictionError::InvalidModel)?;
    }

    comparator.update_best();
    let baseline = comparator.entry(0);
    let candidate = comparator.entry(1);
    let validation_samples = candidate.map_or(0, |entry| entry.total_samples());
    let baseline_samples = baseline.map_or(0, |entry| entry.total_samples());
    let baseline_mae = baseline.and_then(|entry| entry.rolling_mae());
    let candidate_mae = candidate.and_then(|entry| entry.rolling_mae());
    let training_samples = model.samples_seen();

    if training_samples < config.min_training_samples {
        return Ok(fallback(
            "insufficientTrainingData",
            training_samples,
            validation_samples,
            baseline_mae,
            candidate_mae,
        ));
    }
    if validation_samples < config.min_validation_samples || baseline_samples != validation_samples
    {
        return Ok(fallback(
            "insufficientValidationData",
            training_samples,
            validation_samples,
            baseline_mae,
            candidate_mae,
        ));
    }
    let (Some(baseline_error), Some(candidate_error)) = (baseline_mae, candidate_mae) else {
        return Ok(fallback(
            "qualityMetricsUnavailable",
            training_samples,
            validation_samples,
            baseline_mae,
            candidate_mae,
        ));
    };
    if candidate_error >= baseline_error * config.required_error_ratio {
        return Ok(fallback(
            "candidateNotBetter",
            training_samples,
            validation_samples,
            baseline_mae,
            candidate_mae,
        ));
    }

    let recent_rate = weighted_baseline_rate(observations, now, config.baseline_decay_tau_hours);
    let predicted_rate = model
        .predict(&features(current_percentage, now, recent_rate))
        .map_err(|_| BatteryPredictionError::InvalidModel)?;
    if !predicted_rate.is_finite()
        || predicted_rate <= 0.0
        || predicted_rate > config.max_drain_per_hour
    {
        return Ok(fallback(
            "candidateOutsideSafetyBounds",
            training_samples,
            validation_samples,
            baseline_mae,
            candidate_mae,
        ));
    }
    let remaining_hours = current_percentage as f64 / predicted_rate;
    if !remaining_hours.is_finite() || remaining_hours > config.max_remaining_hours {
        return Ok(fallback(
            "candidateOutsideSafetyBounds",
            training_samples,
            validation_samples,
            baseline_mae,
            candidate_mae,
        ));
    }

    Ok(BatteryPredictionOutput {
        remaining_hours: Some(remaining_hours),
        source: PredictionSource::LocalAi,
        reason: "candidatePassedQualityGate".into(),
        training_samples,
        validation_samples,
        baseline_mae,
        candidate_mae,
    })
}

fn fallback(
    reason: &str,
    training_samples: u64,
    validation_samples: u64,
    baseline_mae: Option<f64>,
    candidate_mae: Option<f64>,
) -> BatteryPredictionOutput {
    BatteryPredictionOutput {
        remaining_hours: None,
        source: PredictionSource::BaselineRecommended,
        reason: reason.into(),
        training_samples,
        validation_samples,
        baseline_mae,
        candidate_mae,
    }
}

fn weighted_baseline_rate(
    observations: &[DrainObservation],
    at: DateTime<Utc>,
    decay_tau_hours: f64,
) -> Option<f64> {
    let mut weighted_rate = 0.0;
    let mut total_weight = 0.0;
    for observation in observations {
        let hours_ago = (at - observation.ended_at).num_seconds().max(0) as f64 / 3600.0;
        let weight = (-hours_ago / decay_tau_hours).exp();
        weighted_rate += observation.drain_per_hour * weight;
        total_weight += weight;
    }
    (total_weight > 0.0).then_some(weighted_rate / total_weight)
}

fn features(percentage: u8, at: DateTime<Utc>, recent_rate: Option<f64>) -> [f64; 6] {
    let local = at.with_timezone(&Local);
    let hour_angle = local.hour() as f64 / 24.0 * std::f64::consts::TAU;
    let weekday_angle = local.weekday().num_days_from_monday() as f64 / 7.0 * std::f64::consts::TAU;
    [
        percentage as f64 / 100.0,
        hour_angle.sin(),
        hour_angle.cos(),
        weekday_angle.sin(),
        weekday_angle.cos(),
        recent_rate.unwrap_or(1.0) / 10.0,
    ]
}

fn discharge_observations(
    samples: &[BatterySampleInput],
    config: &BatteryModelConfig,
) -> Vec<DrainObservation> {
    let mut observations = Vec::new();
    let mut segment: Vec<&BatterySampleInput> = Vec::new();
    let mut previous: Option<&BatterySampleInput> = None;
    for sample in samples {
        let split = previous.is_some_and(|prev| {
            prev.charging
                || sample.charging
                || sample.at_unix_ms - prev.at_unix_ms
                    > config.session_gap_minutes.saturating_mul(60_000)
                || sample.percentage.saturating_sub(prev.percentage)
                    >= config.replacement_rise_percent
        });
        if split {
            finish_segment(&segment, &mut observations, config);
            segment.clear();
        }
        if !sample.charging {
            segment.push(sample);
        }
        previous = Some(sample);
    }
    // The current, unfinished segment is input context, never its own label.
    observations
}

fn finish_segment(
    segment: &[&BatterySampleInput],
    observations: &mut Vec<DrainObservation>,
    config: &BatteryModelConfig,
) {
    let (Some(start), Some(end)) = (segment.first(), segment.last()) else {
        return;
    };
    let drop = start.percentage as f64 - end.percentage as f64;
    if drop < config.min_drop_percent {
        return;
    }
    let hours = (end.at_unix_ms - start.at_unix_ms) as f64 / 3_600_000.0;
    let rate = drop / hours;
    if !hours.is_finite()
        || hours <= 0.0
        || !rate.is_finite()
        || rate <= 0.0
        || rate > config.max_drain_per_hour
    {
        return;
    }
    let (Some(at), Some(ended_at)) = (
        DateTime::from_timestamp_millis(start.at_unix_ms),
        DateTime::from_timestamp_millis(end.at_unix_ms),
    ) else {
        return;
    };
    observations.push(DrainObservation {
        at,
        ended_at,
        percentage: start.percentage,
        drain_per_hour: rate,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn sample(at: DateTime<Utc>, percentage: u8, charging: bool) -> BatterySampleInput {
        BatterySampleInput {
            at_unix_ms: at.timestamp_millis(),
            percentage,
            charging,
        }
    }

    #[test]
    fn cold_start_explicitly_recommends_baseline() {
        let now = Utc::now();
        let result = predict(
            &BatteryPredictionInput {
                now_unix_ms: now.timestamp_millis(),
                samples: vec![sample(now, 80, false)],
            },
            &BatteryModelConfig::default(),
        )
        .unwrap();
        assert_eq!(result.source, PredictionSource::BaselineRecommended);
        assert_eq!(result.remaining_hours, None);
    }

    #[test]
    fn learned_daily_pattern_can_pass_quality_gate() {
        let start = Utc.with_ymd_and_hms(2026, 1, 5, 0, 0, 0).unwrap();
        let observations = (0..120)
            .map(|index| {
                let at = start + Duration::hours(index);
                let angle = at.hour() as f64 / 24.0 * std::f64::consts::TAU;
                DrainObservation {
                    at,
                    ended_at: at + Duration::minutes(30),
                    percentage: 80,
                    drain_per_hour: 5.0 + 3.0 * angle.sin() + 1.5 * angle.cos(),
                }
            })
            .collect::<Vec<_>>();
        let result = validated_model_prediction(
            &observations,
            80,
            start + Duration::hours(121),
            &BatteryModelConfig::default(),
        )
        .unwrap();
        assert_eq!(result.source, PredictionSource::LocalAi);
        assert!(result.remaining_hours.is_some_and(f64::is_finite));

        let capped = validated_model_prediction(
            &observations,
            80,
            start + Duration::hours(121),
            &BatteryModelConfig {
                max_remaining_hours: 0.1,
                ..BatteryModelConfig::default()
            },
        )
        .unwrap();
        assert_eq!(capped.source, PredictionSource::BaselineRecommended);
        assert_eq!(capped.remaining_hours, None);
    }

    #[test]
    fn future_samples_never_enter_training() {
        let now = Utc.with_ymd_and_hms(2026, 7, 13, 12, 0, 0).unwrap();
        let result = predict(
            &BatteryPredictionInput {
                now_unix_ms: now.timestamp_millis(),
                samples: vec![
                    sample(now, 80, false),
                    sample(now + Duration::minutes(1), 100, false),
                    sample(now + Duration::minutes(6), 90, false),
                    sample(now + Duration::minutes(20), 90, false),
                ],
            },
            &BatteryModelConfig::default(),
        )
        .unwrap();
        assert_eq!(result.training_samples, 0);
        assert_eq!(result.source, PredictionSource::BaselineRecommended);
    }

    #[test]
    fn invalid_percentage_is_rejected() {
        let now = Utc::now();
        let error = predict(
            &BatteryPredictionInput {
                now_unix_ms: now.timestamp_millis(),
                samples: vec![sample(now, 101, false)],
            },
            &BatteryModelConfig::default(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            BatteryPredictionError::InvalidSample { index: 0 }
        ));
    }
}
