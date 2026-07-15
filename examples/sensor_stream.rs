//! Sensor stream anomaly detection example.
//!
//! Simulates a temperature sensor with normal variation, slow drift, and a
//! sudden anomaly. Uses rolling statistics and a simple z-score to flag
//! unusual readings.

use rand::SeedableRng;
use rill_ml::OnlineStatistic;
use rill_ml::stats::{RollingMean, RollingVariance, VarianceKind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let window = 30;
    let mut rolling_mean = RollingMean::new(window)?;
    let mut rolling_var = RollingVariance::new(window, VarianceKind::Population)?;

    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(77);
    let n = 300;

    println!("=== Sensor stream anomaly detection ===");
    println!("Window size: {window}\n");

    let mut anomalies = 0;
    for i in 0..n {
        // Normal temperature around 22°C with noise.
        let mut temp = 22.0 + rand::Rng::gen_range(&mut rng, -0.5..0.5);

        // Slow drift starting at t=100.
        if (100..200).contains(&i) {
            temp += (i - 100) as f64 * 0.02;
        }

        // Sudden anomaly at t=220.
        if i == 220 {
            temp += 5.0;
        }

        rolling_mean.update(temp)?;
        rolling_var.update(temp)?;

        if let (Some(mean), Some(std)) = (rolling_mean.value(), rolling_var.std_dev())
            && std > 1e-9
        {
            let z = (temp - mean).abs() / std;
            if z > 3.0 && rolling_mean.len() >= window {
                anomalies += 1;
                println!(
                    "  [t={i:3}] ANOMALY: temp={temp:.2}°C, mean={mean:.2}, std={std:.3}, z={z:.2}"
                );
            }
        }
    }

    println!("\nTotal anomalies flagged: {anomalies}");
    Ok(())
}
