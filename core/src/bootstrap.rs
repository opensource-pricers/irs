/// Discount factor bootstrap from par OIS swap rates.
/// Per SPEC-003 §2.
///
/// The bootstrap solves the par swap condition at each tenor:
///   S[n] × Σ(τ[i] × DF[i], i=1..n) = 1 - DF[n]
///
/// Rearranging:
///   DF[n] = (1 - S[n] × annuitySum) / (1 + S[n] × τ[n])
///
/// Solved sequentially. No iteration required. Algebraically exact.

use crate::math::{Ray, RAY, ray_mul, ray_div};
use crate::conventions::Currency;
use crate::daycount::{year_fraction, year_fractions};

/// Error types for bootstrap.
#[derive(Debug, PartialEq)]
pub enum BootstrapError {
    /// No rates provided.
    EmptyInput,
    /// Payment dates length doesn't match rates × payments_per_year.
    LengthMismatch { expected: usize, got: usize },
    /// Computed DF is zero or negative (shouldn't happen for positive rates).
    InvalidDF { period: usize },
    /// Computed DF > RAY (negative rates — not supported in MVP).
    NegativeRateDetected { period: usize },
}

/// Bootstrap discount factors from par OIS swap rates.
///
/// # Arguments
/// * `swap_rates` — Par OIS rates in RAY, one per tenor year.
/// * `payment_dates` — Unix timestamps for each payment date, ascending.
///   For annual currencies: length == swap_rates.len()
///   For semi-annual: length == 2 × swap_rates.len()
/// * `settlement` — Unix timestamp of the valuation date.
/// * `currency` — Determines day count and payment frequency.
///
/// # Returns
/// Discount factors in RAY, one per payment date.
pub fn bootstrap_ois(
    swap_rates: &[Ray],
    payment_dates: &[u64],
    settlement: u64,
    currency: Currency,
) -> Result<Vec<Ray>, BootstrapError> {
    let conv = currency.convention();
    let n_tenors = swap_rates.len();
    let n_payments = payment_dates.len();
    let ppy = conv.payments_per_year as usize;

    if n_tenors == 0 {
        return Err(BootstrapError::EmptyInput);
    }
    if n_payments != n_tenors * ppy {
        return Err(BootstrapError::LengthMismatch {
            expected: n_tenors * ppy,
            got: n_payments,
        });
    }

    // Verify payment dates are ascending
    for i in 1..payment_dates.len() {
        if payment_dates[i] <= payment_dates[i-1] {
            return Err(BootstrapError::InvalidDF { period: i });
        }
    }
    // Verify all dates after settlement
    if payment_dates[0] <= settlement {
        return Err(BootstrapError::InvalidDF { period: 0 });
    }

    // Compute year fractions
    let taus = year_fractions(conv.day_count, settlement, payment_dates);

    // Expand swap rates to per-period
    // For annual: rate[i] applies to period i
    // For semi-annual: rate[k] applies to periods 2k and 2k+1
    let mut expanded_rates = Vec::with_capacity(n_payments);
    for k in 0..n_tenors {
        for _ in 0..ppy {
            expanded_rates.push(swap_rates[k]);
        }
    }

    // Core bootstrap
    let mut dfs = Vec::with_capacity(n_payments);
    let mut annuity_sum: Ray = 0;

    for i in 0..n_payments {
        let s = expanded_rates[i];
        let tau = taus[i];

        // numerator = 1 - S × annuitySum
        let s_times_annuity = ray_mul(s, annuity_sum);
        if s_times_annuity > RAY {
            return Err(BootstrapError::InvalidDF { period: i });
        }
        let numerator = RAY - s_times_annuity;

        // denominator = 1 + S × τ
        let denominator = RAY + ray_mul(s, tau);

        // DF = numerator / denominator
        let df = ray_div(numerator, denominator);

        if df == 0 {
            return Err(BootstrapError::InvalidDF { period: i });
        }
        if df > RAY {
            return Err(BootstrapError::NegativeRateDetected { period: i });
        }

        dfs.push(df);
        annuity_sum += ray_mul(tau, df);
    }

    Ok(dfs)
}

/// Convenience: bootstrap with uniform annual periods (τ=1 for all).
pub fn bootstrap_annual(swap_rates: &[Ray]) -> Result<Vec<Ray>, BootstrapError> {
    if swap_rates.is_empty() {
        return Err(BootstrapError::EmptyInput);
    }

    let mut dfs = Vec::with_capacity(swap_rates.len());
    let mut annuity_sum: Ray = 0;

    for (i, &s) in swap_rates.iter().enumerate() {
        let s_times_annuity = ray_mul(s, annuity_sum);
        if s_times_annuity > RAY {
            return Err(BootstrapError::InvalidDF { period: i });
        }
        let numerator = RAY - s_times_annuity;
        let denominator = RAY + s; // τ=1, so ray_mul(s, RAY) = s

        let df = ray_div(numerator, denominator);
        if df == 0 {
            return Err(BootstrapError::InvalidDF { period: i });
        }
        if df > RAY {
            return Err(BootstrapError::NegativeRateDetected { period: i });
        }

        dfs.push(df);
        annuity_sum += df; // τ=1
    }

    Ok(dfs)
}

/// Compute the forward rate between two dates from discount factors.
/// F(T1,T2) = (DF(T1)/DF(T2) - 1) / τ(T1,T2)
pub fn forward_rate(df_start: Ray, df_end: Ray, tau: Ray) -> Ray {
    if df_end == 0 || tau == 0 {
        return 0;
    }
    let ratio = ray_div(df_start, df_end); // DF1/DF2 in RAY
    let ratio_minus_one = ratio - RAY; // (DF1/DF2 - 1) in RAY
    ray_div(ratio_minus_one, tau)
}

/// Compute par swap rate from discount factors.
/// S = (1 - DF[n]) / Σ(τ[i] × DF[i])
pub fn par_swap_rate(dfs: &[Ray], taus: &[Ray]) -> Ray {
    assert_eq!(dfs.len(), taus.len());
    let numerator = RAY - dfs[dfs.len() - 1];
    let mut annuity: Ray = 0;
    for i in 0..dfs.len() {
        annuity += ray_mul(taus[i], dfs[i]);
    }
    ray_div(numerator, annuity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{bps_to_ray, ray_to_f64};

    #[test]
    fn test_bootstrap_flat_5pct() {
        let rates: Vec<Ray> = vec![bps_to_ray(500); 5];
        let dfs = bootstrap_annual(&rates).unwrap();

        assert_eq!(dfs.len(), 5);

        // DF(1Y) should be 1/1.05 ≈ 0.952381
        let df1 = ray_to_f64(dfs[0]);
        assert!((df1 - 0.952381).abs() < 1e-5, "DF(1Y) = {}", df1);

        // All decreasing
        for i in 1..5 {
            assert!(dfs[i] < dfs[i - 1], "DFs must decrease");
        }

        // Par condition: S * Σ(DF) = 1 - DF(last)
        let annuity: Ray = dfs.iter().sum();
        let lhs = ray_mul(bps_to_ray(500), annuity);
        let rhs = RAY - dfs[4];
        let diff = if lhs > rhs { lhs - rhs } else { rhs - lhs };
        assert!(diff < 1_000_000_000, "Par condition residual too large: {}", diff);
    }

    #[test]
    fn test_bootstrap_flat_10pct() {
        let rates: Vec<Ray> = vec![bps_to_ray(1000); 5];
        let dfs = bootstrap_annual(&rates).unwrap();
        let df1 = ray_to_f64(dfs[0]);
        assert!((df1 - 1.0 / 1.1).abs() < 1e-10);
    }

    #[test]
    fn test_bootstrap_upward_curve() {
        let rates: Vec<Ray> = vec![
            bps_to_ray(200), bps_to_ray(300), bps_to_ray(400),
            bps_to_ray(450), bps_to_ray(500),
        ];
        let dfs = bootstrap_annual(&rates).unwrap();
        for i in 1..5 {
            assert!(dfs[i] < dfs[i - 1]);
        }
        // Verify par condition at each tenor
        for t in 0..5 {
            let annuity: Ray = dfs[..=t].iter().sum();
            let lhs = ray_mul(rates[t], annuity);
            let rhs = RAY - dfs[t];
            let diff = if lhs > rhs { lhs - rhs } else { rhs - lhs };
            assert!(diff < 1_000_000_000, "Tenor {} par residual: {}", t + 1, diff);
        }
    }

    #[test]
    fn test_bootstrap_very_high_rates() {
        let rates: Vec<Ray> = vec![bps_to_ray(5000); 3]; // 50%
        let dfs = bootstrap_annual(&rates).unwrap();
        let df1 = ray_to_f64(dfs[0]);
        assert!((df1 - 2.0 / 3.0).abs() < 1e-10, "DF at 50% should be 2/3");
    }

    #[test]
    fn test_bootstrap_near_zero() {
        let rates: Vec<Ray> = vec![bps_to_ray(1); 10]; // 0.01%
        let dfs = bootstrap_annual(&rates).unwrap();
        assert!(ray_to_f64(dfs[9]) > 0.999, "DF(10Y) at 1bp should be > 0.999");
    }

    #[test]
    fn test_bootstrap_empty_error() {
        let result = bootstrap_annual(&[]);
        assert_eq!(result, Err(BootstrapError::EmptyInput));
    }

    #[test]
    fn test_bootstrap_ois_with_dates() {
        let day: u64 = 86400;
        let settle: u64 = 1_773_964_800;
        let dates: Vec<u64> = (1..=5).map(|i| settle + i * 365 * day).collect();
        let rates: Vec<Ray> = vec![bps_to_ray(400); 5];

        let dfs = bootstrap_ois(&rates, &dates, settle, Currency::GBP).unwrap();
        assert_eq!(dfs.len(), 5);

        // GBP = ACT/365F, annual. With 365-day periods, τ=1.0.
        // Should match bootstrap_annual exactly.
        let dfs_annual = bootstrap_annual(&rates).unwrap();
        for i in 0..5 {
            assert_eq!(dfs[i], dfs_annual[i], "OIS and annual should match for GBP");
        }
    }

    #[test]
    fn test_bootstrap_ois_semi_annual() {
        let day: u64 = 86400;
        let settle: u64 = 1_773_964_800;
        // 3 tenors × 2 payments = 6 dates
        let dates: Vec<u64> = (1..=6).map(|i| settle + i * 182 * day).collect();
        let rates: Vec<Ray> = vec![bps_to_ray(400); 3];

        let dfs = bootstrap_ois(&rates, &dates, settle, Currency::AUD).unwrap();
        assert_eq!(dfs.len(), 6);
        for i in 1..6 {
            assert!(dfs[i] < dfs[i - 1], "DFs must decrease");
        }
    }

    #[test]
    fn test_bootstrap_ois_length_mismatch() {
        let settle: u64 = 1_773_964_800;
        let dates: Vec<u64> = vec![settle + 86400 * 365]; // 1 date
        let rates: Vec<Ray> = vec![bps_to_ray(400); 3]; // 3 rates

        let result = bootstrap_ois(&rates, &dates, settle, Currency::GBP);
        assert!(matches!(result, Err(BootstrapError::LengthMismatch { .. })));
    }

    #[test]
    fn test_forward_rate() {
        let rates: Vec<Ray> = vec![bps_to_ray(300); 5];
        let dfs = bootstrap_annual(&rates).unwrap();
        // Forward rate between year 2 and year 3 should be ~3% for flat curve
        let fwd = forward_rate(dfs[1], dfs[2], RAY);
        let fwd_pct = ray_to_f64(fwd) * 100.0;
        assert!((fwd_pct - 3.0).abs() < 0.01, "Fwd 2-3Y = {}%", fwd_pct);
    }

    #[test]
    fn test_par_swap_rate() {
        let rate = bps_to_ray(400);
        let rates: Vec<Ray> = vec![rate; 5];
        let dfs = bootstrap_annual(&rates).unwrap();
        let taus: Vec<Ray> = vec![RAY; 5];
        let par = par_swap_rate(&dfs, &taus);
        let diff = if par > rate { par - rate } else { rate - par };
        assert!(diff < 1_000_000_000, "Par rate should match input: diff = {}", diff);
    }
}
