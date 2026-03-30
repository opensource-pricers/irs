/// Stress scenarios per SPEC-006 §5.2.
use crate::math::{Ray, ONE_BP};
use crate::bootstrap::bootstrap_annual;
use crate::cashflow::{Cashflow, value_cashflows};

/// Pre-defined stress scenarios.
#[derive(Debug, Clone, Copy)]
pub enum Scenario {
    ParallelUp100,
    ParallelDown100,
    ParallelUp200,
    ParallelDown200,
    Steepening,    // ≤2Y: -50bp, ≥10Y: +50bp, linear between
    Flattening,    // ≤2Y: +50bp, ≥10Y: -50bp, linear between
    ShortEndShock, // ≤2Y: +300bp, >2Y: 0
    LongEndShock,  // <10Y: 0, ≥10Y: +200bp
}

/// Result of a stress scenario.
#[derive(Debug)]
pub struct StressResult {
    pub scenario: &'static str,
    pub base_pv: i128,
    pub stressed_pv: i128,
    pub pnl: i128,
}

/// Apply a scenario bump to rates.
/// `tenor_years[i]` is the tenor in years for rate[i].
pub fn apply_scenario(rates: &[Ray], tenor_years: &[f64], scenario: Scenario) -> Vec<Ray> {
    rates.iter().enumerate().map(|(i, &r)| {
        let t = tenor_years[i];
        let bump: i128 = match scenario {
            Scenario::ParallelUp100 => 100 * ONE_BP as i128,
            Scenario::ParallelDown100 => -(100 * ONE_BP as i128),
            Scenario::ParallelUp200 => 200 * ONE_BP as i128,
            Scenario::ParallelDown200 => -(200 * ONE_BP as i128),
            Scenario::Steepening => {
                if t <= 2.0 { -(50 * ONE_BP as i128) }
                else if t >= 10.0 { 50 * ONE_BP as i128 }
                else { ((-50.0 + (t - 2.0) / 8.0 * 100.0) * ONE_BP as f64) as i128 }
            }
            Scenario::Flattening => {
                if t <= 2.0 { 50 * ONE_BP as i128 }
                else if t >= 10.0 { -(50 * ONE_BP as i128) }
                else { ((50.0 - (t - 2.0) / 8.0 * 100.0) * ONE_BP as f64) as i128 }
            }
            Scenario::ShortEndShock => {
                if t <= 2.0 { 300 * ONE_BP as i128 } else { 0 }
            }
            Scenario::LongEndShock => {
                if t >= 10.0 { 200 * ONE_BP as i128 } else { 0 }
            }
        };
        if bump >= 0 {
            r + bump as u128
        } else {
            r.saturating_sub((-bump) as u128)
        }
    }).collect()
}

/// Run multiple stress scenarios.
pub fn run_stress(
    cashflows: &[Cashflow],
    rates: &[Ray],
    tenor_years: &[f64],
    settlement: u64,
    dates: &[u64],
    scenarios: &[Scenario],
) -> Vec<StressResult> {
    let dfs_base = bootstrap_annual(rates).unwrap();
    let base_pv = value_cashflows(cashflows, settlement, dates, &dfs_base);

    scenarios.iter().map(|&scenario| {
        let bumped = apply_scenario(rates, tenor_years, scenario);
        let dfs_bumped = bootstrap_annual(&bumped).unwrap();
        let stressed_pv = value_cashflows(cashflows, settlement, dates, &dfs_bumped);
        StressResult {
            scenario: scenario_name(scenario),
            base_pv,
            stressed_pv,
            pnl: stressed_pv - base_pv,
        }
    }).collect()
}

fn scenario_name(s: Scenario) -> &'static str {
    match s {
        Scenario::ParallelUp100 => "PARALLEL_UP_100",
        Scenario::ParallelDown100 => "PARALLEL_DOWN_100",
        Scenario::ParallelUp200 => "PARALLEL_UP_200",
        Scenario::ParallelDown200 => "PARALLEL_DOWN_200",
        Scenario::Steepening => "STEEPENING",
        Scenario::Flattening => "FLATTENING",
        Scenario::ShortEndShock => "SHORT_END_SHOCK",
        Scenario::LongEndShock => "LONG_END_SHOCK",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::bps_to_ray;
    use crate::products::irs;
    use crate::leg::build_trade;
    use crate::conventions::Currency;
    use crate::schedule::ymd_to_unix;

    #[test]
    fn test_parallel_up_consistent_with_dv01() {
        let start = ymd_to_unix(2026, 3, 20);
        let end = ymd_to_unix(2031, 3, 20);
        let rate = bps_to_ray(400);
        let legs = irs(Currency::GBP, 10_000_000 * RAY, rate, start, end, true);
        let cfs = build_trade(&legs);
        let rates = vec![rate; 5];
        let tenors = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let dates: Vec<u64> = (1..=5).map(|i| start + i * 365 * 86400).collect();

        let results = run_stress(&cfs, &rates, &tenors, start, &dates,
            &[Scenario::ParallelUp100, Scenario::ParallelDown100]);

        // Payer profits from rate rises
        assert!(results[0].pnl > 0, "Payer should profit from +100bp");
        assert!(results[1].pnl < 0, "Payer should lose from -100bp");

        // PnL should be roughly symmetric
        let ratio = results[0].pnl as f64 / (-results[1].pnl as f64);
        assert!((ratio - 1.0).abs() < 0.1, "PnL should be roughly symmetric: {}", ratio);
    }

    #[test]
    fn test_all_8_scenarios_run() {
        let start = ymd_to_unix(2026, 3, 20);
        let end = ymd_to_unix(2031, 3, 20);
        let rate = bps_to_ray(300);
        let legs = irs(Currency::GBP, 10_000_000 * RAY, rate, start, end, true);
        let cfs = build_trade(&legs);
        let rates = vec![rate; 5];
        let tenors = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let dates: Vec<u64> = (1..=5).map(|i| start + i * 365 * 86400).collect();

        let all = vec![
            Scenario::ParallelUp100, Scenario::ParallelDown100,
            Scenario::ParallelUp200, Scenario::ParallelDown200,
            Scenario::Steepening, Scenario::Flattening,
            Scenario::ShortEndShock, Scenario::LongEndShock,
        ];

        let results = run_stress(&cfs, &rates, &tenors, start, &dates, &all);
        assert_eq!(results.len(), 8);
        // All parallel/steepening/flattening/short-end should move PV.
        // LONG_END_SHOCK may not affect a 5Y swap (bump only hits ≥10Y).
        for r in &results {
            if r.scenario != "LONG_END_SHOCK" {
                assert_ne!(r.base_pv, r.stressed_pv, "{} should move PV", r.scenario);
            }
        }
    }
}
