/// Cashflow primitives per SPEC-004.
/// Four atomic types: FIXED, FLOATING, NOTIONAL_EXCHANGE, CONDITIONAL.
use crate::math::{Ray, RAY, ray_mul};
use crate::interpolation::interpolate_df;
use ethnum::I256;

/// Cashflow type.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum CashflowType {
    Fixed = 0,
    Floating = 1,
    NotionalExchange = 2,
    Conditional = 3,
}

/// A single cashflow.
#[derive(Debug, Clone)]
pub struct Cashflow {
    pub cf_type: CashflowType,
    /// +1 = receive, -1 = pay
    pub direction: i8,
    /// Payment date (Unix timestamp)
    pub payment_date: u64,
    /// Notional in RAY
    pub notional: Ray,
    /// For FIXED: the fixed amount (direction × notional × rate × tau, pre-computed)
    pub fixed_amount: i128,
    /// For FLOATING: accrual period start
    pub accrual_start: u64,
    /// For FLOATING: accrual period end
    pub accrual_end: u64,
    /// For FLOATING: spread over index (in RAY)
    pub spread: i128,
    /// For CONDITIONAL: condition type
    pub condition_met: bool, // MVP: intrinsic only
}

/// Present value of a single cashflow.
///
/// # Arguments
/// * `cf` — The cashflow
/// * `settlement` — Valuation date
/// * `dates` — Curve node timestamps
/// * `dfs` — Discount factors at curve nodes
pub fn value_cashflow(
    cf: &Cashflow,
    settlement: u64,
    dates: &[u64],
    dfs: &[Ray],
) -> i128 {
    let df_payment = interpolate_df(cf.payment_date, settlement, dates, dfs);

    match cf.cf_type {
        CashflowType::Fixed => {
            // PV = amount × DF(paymentDate)
            let pv = ray_mul_signed(cf.fixed_amount, df_payment);
            pv
        }
        CashflowType::Floating => {
            // PV = direction × notional × (DF(accrualStart) - DF(accrualEnd))
            // Uses I256 intermediate to prevent overflow with large notionals.
            let df_start = interpolate_df(cf.accrual_start, settlement, dates, dfs);
            let df_end = interpolate_df(cf.accrual_end, settlement, dates, dfs);
            let dir = I256::from(cf.direction as i64);
            let notional = I256::from(cf.notional);
            let ray = I256::from(RAY);
            let df_diff = I256::from(df_start) - I256::from(df_end);

            let float_pv = (dir * df_diff * notional / ray).as_i128();

            // Spread component
            if cf.spread != 0 {
                let elapsed = I256::from((cf.accrual_end - cf.accrual_start) as i128);
                let tau = elapsed * ray / I256::from(365i64 * 86400);
                let spread = I256::from(cf.spread);
                let df_pay = I256::from(df_payment);
                let spread_pv = (dir * spread * notional / ray * tau / ray * df_pay / ray).as_i128();
                float_pv + spread_pv
            } else {
                float_pv
            }
        }
        CashflowType::NotionalExchange => {
            // PV = direction × notional × DF(paymentDate)
            let dir = I256::from(cf.direction as i64);
            let pv = dir * I256::from(ray_mul(cf.notional, df_payment));
            pv.as_i128()
        }
        CashflowType::Conditional => {
            // MVP: intrinsic value only
            if cf.condition_met {
                ray_mul_signed(cf.fixed_amount, df_payment)
            } else {
                0
            }
        }
    }
}

/// Present value of a collection of cashflows.
pub fn value_cashflows(
    cashflows: &[Cashflow],
    settlement: u64,
    dates: &[u64],
    dfs: &[Ray],
) -> i128 {
    cashflows.iter().map(|cf| value_cashflow(cf, settlement, dates, dfs)).sum()
}

/// Multiply a signed amount by an unsigned DF.
fn ray_mul_signed(amount: i128, df: Ray) -> i128 {
    if amount >= 0 {
        ray_mul(amount as u128, df) as i128
    } else {
        -(ray_mul((-amount) as u128, df) as i128)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{bps_to_ray, ray_to_f64};
    use crate::bootstrap::bootstrap_annual;

    #[test]
    fn test_fixed_cashflow() {
        let rates = vec![bps_to_ray(500); 3];
        let dfs = bootstrap_annual(&rates).unwrap();
        let settle: u64 = 1_773_964_800;
        let day: u64 = 86400;
        let dates: Vec<u64> = (1..=3).map(|i| settle + i * 365 * day).collect();

        // Fixed coupon: receive 50,000 at year 1
        let cf = Cashflow {
            cf_type: CashflowType::Fixed,
            direction: 1,
            payment_date: dates[0],
            notional: 1_000_000 * RAY,
            fixed_amount: 50_000 * RAY as i128,
            accrual_start: 0,
            accrual_end: 0,
            spread: 0,
            condition_met: false,
        };

        let pv = value_cashflow(&cf, settle, &dates, &dfs);
        let pv_f64 = pv as f64 / RAY as f64;
        // PV ≈ 50,000 × 0.9524 ≈ 47,619
        assert!((pv_f64 - 47_619.0).abs() < 10.0, "PV = {}", pv_f64);
    }

    #[test]
    fn test_floating_cashflow() {
        let rates = vec![bps_to_ray(500); 3];
        let dfs = bootstrap_annual(&rates).unwrap();
        let settle: u64 = 1_773_964_800;
        let day: u64 = 86400;
        let dates: Vec<u64> = (1..=3).map(|i| settle + i * 365 * day).collect();

        // Floating: receive floating over year 1
        let cf = Cashflow {
            cf_type: CashflowType::Floating,
            direction: 1,
            payment_date: dates[0],
            notional: 1_000_000 * RAY,
            fixed_amount: 0,
            accrual_start: settle,
            accrual_end: dates[0],
            spread: 0,
            condition_met: false,
        };

        let pv = value_cashflow(&cf, settle, &dates, &dfs);
        let pv_f64 = pv as f64 / RAY as f64;
        // Floating PV for year 1 = N × (1 - DF(1Y)) ≈ 1M × (1 - 0.9524) ≈ 47,619
        assert!((pv_f64 - 47_619.0).abs() < 10.0, "Floating PV = {}", pv_f64);
    }

    #[test]
    fn test_notional_exchange() {
        let rates = vec![bps_to_ray(500); 3];
        let dfs = bootstrap_annual(&rates).unwrap();
        let settle: u64 = 1_773_964_800;
        let day: u64 = 86400;
        let dates: Vec<u64> = (1..=3).map(|i| settle + i * 365 * day).collect();

        let cf = Cashflow {
            cf_type: CashflowType::NotionalExchange,
            direction: 1,
            payment_date: dates[2], // year 3
            notional: 1_000_000 * RAY,
            fixed_amount: 0,
            accrual_start: 0,
            accrual_end: 0,
            spread: 0,
            condition_met: false,
        };

        let pv = value_cashflow(&cf, settle, &dates, &dfs);
        let pv_f64 = pv as f64 / RAY as f64;
        // PV = 1M × DF(3Y) ≈ 1M × 0.8638 ≈ 863,838
        assert!(pv_f64 > 860_000.0 && pv_f64 < 870_000.0, "Notional PV = {}", pv_f64);
    }
}
