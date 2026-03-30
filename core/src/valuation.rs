/// Trade valuation and risk measures per SPEC-005.
use crate::math::{Ray, ONE_BP};
use crate::bootstrap::bootstrap_annual;
use crate::cashflow::{Cashflow, value_cashflows};

/// Present value of a trade (collection of cashflows).
pub fn present_value(
    cashflows: &[Cashflow],
    settlement: u64,
    dates: &[u64],
    dfs: &[Ray],
) -> i128 {
    value_cashflows(cashflows, settlement, dates, dfs)
}

/// DV01 — central difference (SPEC-005 §5.4).
/// DV01 = (PV(rates+1bp) - PV(rates-1bp)) / 2
pub fn dv01(
    cashflows: &[Cashflow],
    rates: &[Ray],
    settlement: u64,
    dates: &[u64],
) -> i128 {
    let rates_up: Vec<Ray> = rates.iter().map(|&r| r + ONE_BP).collect();
    let rates_down: Vec<Ray> = rates.iter().map(|&r| r.saturating_sub(ONE_BP)).collect();

    let dfs_up = bootstrap_annual(&rates_up).unwrap();
    let dfs_down = bootstrap_annual(&rates_down).unwrap();

    let pv_up = value_cashflows(cashflows, settlement, dates, &dfs_up);
    let pv_down = value_cashflows(cashflows, settlement, dates, &dfs_down);

    (pv_up - pv_down) / 2
}

/// Bucket DV01 — per-tenor sensitivity (SPEC-005 §5.4).
pub fn bucket_dv01(
    cashflows: &[Cashflow],
    rates: &[Ray],
    settlement: u64,
    dates: &[u64],
) -> Vec<i128> {
    let dfs_base = bootstrap_annual(rates).unwrap();
    let _pv_base = value_cashflows(cashflows, settlement, dates, &dfs_base);

    let mut buckets = Vec::with_capacity(rates.len());
    for k in 0..rates.len() {
        let mut rates_up = rates.to_vec();
        let mut rates_down = rates.to_vec();
        rates_up[k] += ONE_BP;
        rates_down[k] = rates_down[k].saturating_sub(ONE_BP);

        let dfs_up = bootstrap_annual(&rates_up).unwrap();
        let dfs_down = bootstrap_annual(&rates_down).unwrap();

        let pv_up = value_cashflows(cashflows, settlement, dates, &dfs_up);
        let pv_down = value_cashflows(cashflows, settlement, dates, &dfs_down);

        buckets.push((pv_up - pv_down) / 2);
    }
    buckets
}

/// Gamma — second derivative of PV w.r.t. parallel rate shift.
pub fn gamma(
    cashflows: &[Cashflow],
    rates: &[Ray],
    settlement: u64,
    dates: &[u64],
) -> i128 {
    let dfs_base = bootstrap_annual(rates).unwrap();
    let rates_up: Vec<Ray> = rates.iter().map(|&r| r + ONE_BP).collect();
    let rates_down: Vec<Ray> = rates.iter().map(|&r| r.saturating_sub(ONE_BP)).collect();

    let dfs_up = bootstrap_annual(&rates_up).unwrap();
    let dfs_down = bootstrap_annual(&rates_down).unwrap();

    let pv_base = value_cashflows(cashflows, settlement, dates, &dfs_base);
    let pv_up = value_cashflows(cashflows, settlement, dates, &dfs_up);
    let pv_down = value_cashflows(cashflows, settlement, dates, &dfs_down);

    pv_up + pv_down - 2 * pv_base
}

/// Theta — 1-day carry and roll-down per SPEC-005 §5.4.
///
/// Theta = PV(settlement + 1 day, same rates) - PV(settlement, same rates)
/// Cashflows with payment_date <= settlement' are excluded (rolled off).
pub fn theta(
    cashflows: &[Cashflow],
    rates: &[Ray],
    settlement: u64,
    dates: &[u64],
) -> i128 {
    let dfs = bootstrap_annual(rates).unwrap();
    let pv_base = value_cashflows(cashflows, settlement, dates, &dfs);

    let settlement_tomorrow = settlement + 86400;
    // Filter out cashflows that have rolled off
    let remaining: Vec<Cashflow> = cashflows.iter()
        .filter(|cf| cf.payment_date > settlement_tomorrow)
        .cloned()
        .collect();
    let pv_tomorrow = value_cashflows(&remaining, settlement_tomorrow, dates, &dfs);

    pv_tomorrow - pv_base
}

/// Value a trade built from leg descriptors.
pub fn value_trade(
    legs: &[crate::leg::LegDescriptor],
    settlement: u64,
    dates: &[u64],
    dfs: &[Ray],
) -> i128 {
    let cashflows = crate::leg::build_trade(legs);
    value_cashflows(&cashflows, settlement, dates, dfs)
}

/// Price an entire portfolio in one call.
/// Bootstrap ONCE, price every trade against the same DFs.
/// Returns per-trade PV + aggregate totals.
pub fn value_portfolio(
    trades: &[Vec<crate::leg::LegDescriptor>],
    rates: &[Ray],
    settlement: u64,
    dates: &[u64],
) -> PortfolioResult {
    let dfs = bootstrap_annual(rates).unwrap();

    let mut results = Vec::with_capacity(trades.len());
    let mut total_pv: i128 = 0;

    for (i, legs) in trades.iter().enumerate() {
        let cashflows = crate::leg::build_trade(legs);
        let pv = value_cashflows(&cashflows, settlement, dates, &dfs);
        total_pv += pv;
        results.push(TradeResult { index: i, pv });
    }

    // Portfolio DV01: bump once, reprice everything
    let rates_up: Vec<Ray> = rates.iter().map(|&r| r + ONE_BP).collect();
    let rates_down: Vec<Ray> = rates.iter().map(|&r| r.saturating_sub(ONE_BP)).collect();
    let dfs_up = bootstrap_annual(&rates_up).unwrap();
    let dfs_down = bootstrap_annual(&rates_down).unwrap();

    let mut pv_up: i128 = 0;
    let mut pv_down: i128 = 0;
    for legs in trades {
        let cfs = crate::leg::build_trade(legs);
        pv_up += value_cashflows(&cfs, settlement, dates, &dfs_up);
        pv_down += value_cashflows(&cfs, settlement, dates, &dfs_down);
    }
    let portfolio_dv01 = (pv_up - pv_down) / 2;

    PortfolioResult {
        trades: results,
        total_pv,
        total_dv01: portfolio_dv01,
        num_trades: trades.len(),
    }
}

pub struct TradeResult {
    pub index: usize,
    pub pv: i128,
}

pub struct PortfolioResult {
    pub trades: Vec<TradeResult>,
    pub total_pv: i128,
    pub total_dv01: i128,
    pub num_trades: usize,
}

/// DV01 for a trade built from leg descriptors.
pub fn trade_dv01(
    legs: &[crate::leg::LegDescriptor],
    rates: &[Ray],
    settlement: u64,
    dates: &[u64],
) -> i128 {
    let cashflows = crate::leg::build_trade(legs);
    dv01(&cashflows, rates, settlement, dates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::ray_to_f64;
    use crate::cashflow::CashflowType;

    fn make_irs_cashflows(
        notional: Ray,
        fixed_rate: Ray,
        n_years: usize,
        settlement: u64,
        dates: &[u64],
    ) -> Vec<Cashflow> {
        let mut cfs = Vec::new();
        let day: u64 = 86400;

        for i in 0..n_years {
            // Pay fixed coupon
            let coupon = ray_mul(ray_mul(notional, fixed_rate), RAY); // rate × notional × τ(=1)
            cfs.push(Cashflow {
                cf_type: CashflowType::Fixed,
                direction: -1, // pay
                payment_date: dates[i],
                notional,
                fixed_amount: -(ray_mul(notional, fixed_rate) as i128),
                accrual_start: if i == 0 { settlement } else { dates[i - 1] },
                accrual_end: dates[i],
                spread: 0,
                condition_met: false,
            });

            // Receive floating
            cfs.push(Cashflow {
                cf_type: CashflowType::Floating,
                direction: 1, // receive
                payment_date: dates[i],
                notional,
                fixed_amount: 0,
                accrual_start: if i == 0 { settlement } else { dates[i - 1] },
                accrual_end: dates[i],
                spread: 0,
                condition_met: false,
            });
        }
        cfs
    }

    #[test]
    fn test_irs_at_par() {
        let settle: u64 = 1_773_964_800;
        let day: u64 = 86400;
        let n = 5;
        let dates: Vec<u64> = (1..=n).map(|i| settle + i as u64 * 365 * day).collect();
        let rate = bps_to_ray(400);
        let rates = vec![rate; n];
        let dfs = bootstrap_annual(&rates).unwrap();
        let notional = 10_000_000 * RAY;

        let cfs = make_irs_cashflows(notional, rate, n, settle, &dates);
        let pv = present_value(&cfs, settle, &dates, &dfs);

        // At par, PV should be ~0
        let pv_f64 = pv as f64 / RAY as f64;
        assert!(pv_f64.abs() < 100.0, "PV at par should be ~0, got {}", pv_f64);
    }

    #[test]
    fn test_irs_rates_rise() {
        let settle: u64 = 1_773_964_800;
        let day: u64 = 86400;
        let n = 5;
        let dates: Vec<u64> = (1..=n).map(|i| settle + i as u64 * 365 * day).collect();
        let locked_rate = bps_to_ray(300); // locked at 3%
        let market_rates = vec![bps_to_ray(400); n]; // market at 4%
        let dfs = bootstrap_annual(&market_rates).unwrap();
        let notional = 10_000_000 * RAY;

        let cfs = make_irs_cashflows(notional, locked_rate, n, settle, &dates);
        let pv = present_value(&cfs, settle, &dates, &dfs);

        // Payer locked at 3% in 4% world → positive MTM
        assert!(pv > 0, "Payer profits when rates rise");
    }

    #[test]
    fn test_dv01() {
        let settle: u64 = 1_773_964_800;
        let day: u64 = 86400;
        let n = 5;
        let dates: Vec<u64> = (1..=n).map(|i| settle + i as u64 * 365 * day).collect();
        let rate = bps_to_ray(400);
        let rates = vec![rate; n];
        let notional = 10_000_000 * RAY;

        let cfs = make_irs_cashflows(notional, rate, n, settle, &dates);
        let dv = dv01(&cfs, &rates, settle, &dates);

        // DV01 of 10M 5Y ≈ 10M × ~4.5 × 0.01% ≈ 4,500
        let dv_f64 = dv as f64 / RAY as f64;
        assert!(dv_f64.abs() > 3_000.0 && dv_f64.abs() < 6_000.0,
            "DV01 should be ~4,500, got {}", dv_f64);
    }
}
