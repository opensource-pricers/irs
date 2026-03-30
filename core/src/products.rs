/// Product templates per SPEC-005 §6.
/// Convenience constructors for common instruments.

use crate::math::{Ray, RAY};
use crate::conventions::Currency;
use crate::schedule::Frequency;
use crate::leg::{LegDescriptor, LegType};

/// Vanilla IRS (pay fixed, receive floating or vice versa).
pub fn irs(
    currency: Currency,
    notional: Ray,
    fixed_rate: Ray,
    start: u64,
    end: u64,
    payer: bool, // true = pay fixed
) -> Vec<LegDescriptor> {
    let pay_dir: i8 = if payer { -1 } else { 1 };
    let recv_dir: i8 = -pay_dir;
    vec![
        LegDescriptor::fixed(currency, notional, fixed_rate, start, end, Frequency::Annual, pay_dir),
        LegDescriptor::floating(currency, notional, 0, start, end, Frequency::Annual, recv_dir),
    ]
}

/// Forward Rate Agreement — single-period swap.
pub fn fra(
    currency: Currency,
    notional: Ray,
    fixed_rate: Ray,
    start: u64,
    end: u64,
) -> Vec<LegDescriptor> {
    vec![
        LegDescriptor::fixed(currency, notional, fixed_rate, start, end, Frequency::ZeroCoupon, -1),
        LegDescriptor::floating(currency, notional, 0, start, end, Frequency::ZeroCoupon, 1),
    ]
}

/// Fixed-rate bond (from holder's perspective — receives coupons + principal).
pub fn fixed_rate_bond(
    currency: Currency,
    face: Ray,
    coupon_rate: Ray,
    start: u64,
    maturity: u64,
    frequency: Frequency,
) -> Vec<LegDescriptor> {
    let mut coupon_leg = LegDescriptor::fixed(
        currency, face, coupon_rate, start, maturity, frequency, 1,
    );
    coupon_leg.exchange_notional_end = true; // principal at maturity
    vec![coupon_leg]
}

/// Floating-rate note (from holder's perspective).
pub fn frn(
    currency: Currency,
    face: Ray,
    spread: i128,
    start: u64,
    maturity: u64,
    frequency: Frequency,
) -> Vec<LegDescriptor> {
    let mut float_leg = LegDescriptor::floating(
        currency, face, spread, start, maturity, frequency, 1,
    );
    float_leg.exchange_notional_end = true;
    vec![float_leg]
}

/// Cross-currency swap (pay ccy1 fixed, receive ccy2 floating, with notional exchange).
pub fn xccy_swap(
    ccy_pay: Currency,
    ccy_recv: Currency,
    notional_pay: Ray,
    notional_recv: Ray,
    fixed_rate: Ray,
    start: u64,
    end: u64,
) -> Vec<LegDescriptor> {
    let mut pay_leg = LegDescriptor::fixed(
        ccy_pay, notional_pay, fixed_rate, start, end, Frequency::Annual, -1,
    );
    pay_leg.exchange_notional_start = true;
    pay_leg.exchange_notional_end = true;

    let mut recv_leg = LegDescriptor::floating(
        ccy_recv, notional_recv, 0, start, end, Frequency::Annual, 1,
    );
    recv_leg.exchange_notional_start = true;
    recv_leg.exchange_notional_end = true;

    vec![pay_leg, recv_leg]
}

/// Amortizing swap.
pub fn amortizing_irs(
    currency: Currency,
    notional_schedule: Vec<Ray>,
    fixed_rate: Ray,
    start: u64,
    end: u64,
    payer: bool,
) -> Vec<LegDescriptor> {
    let pay_dir: i8 = if payer { -1 } else { 1 };
    let recv_dir: i8 = -pay_dir;
    let initial = notional_schedule[0];

    let mut fixed_leg = LegDescriptor::fixed(
        currency, initial, fixed_rate, start, end, Frequency::Annual, pay_dir,
    );
    fixed_leg.notional_schedule = Some(notional_schedule.clone());

    let mut float_leg = LegDescriptor::floating(
        currency, initial, 0, start, end, Frequency::Annual, recv_dir,
    );
    float_leg.notional_schedule = Some(notional_schedule);

    vec![fixed_leg, float_leg]
}

/// Zero-coupon swap (single payment at maturity).
pub fn zero_coupon_swap(
    currency: Currency,
    notional: Ray,
    fixed_rate: Ray,
    start: u64,
    end: u64,
    payer: bool,
) -> Vec<LegDescriptor> {
    let pay_dir: i8 = if payer { -1 } else { 1 };
    let recv_dir: i8 = -pay_dir;
    vec![
        LegDescriptor::fixed(currency, notional, fixed_rate, start, end, Frequency::ZeroCoupon, pay_dir),
        LegDescriptor::floating(currency, notional, 0, start, end, Frequency::ZeroCoupon, recv_dir),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::bps_to_ray;
    use crate::leg::build_trade;
    use crate::bootstrap::bootstrap_annual;
    use crate::cashflow::{value_cashflows, CashflowType};
    use crate::schedule::ymd_to_unix;

    #[test]
    fn test_irs_cashflow_count() {
        let start = ymd_to_unix(2026, 3, 20);
        let end = ymd_to_unix(2031, 3, 20);
        let legs = irs(Currency::GBP, 10_000_000 * RAY, bps_to_ray(400), start, end, true);
        let cfs = build_trade(&legs);
        assert_eq!(cfs.len(), 10); // 5 fixed + 5 floating
    }

    #[test]
    fn test_fra_single_period() {
        let start = ymd_to_unix(2026, 3, 20);
        let end = ymd_to_unix(2027, 3, 20);
        let legs = fra(Currency::EUR, 50_000_000 * RAY, bps_to_ray(250), start, end);
        let cfs = build_trade(&legs);
        assert_eq!(cfs.len(), 2); // 1 fixed + 1 floating
    }

    #[test]
    fn test_bond_has_principal() {
        let start = ymd_to_unix(2026, 3, 20);
        let mat = ymd_to_unix(2031, 3, 20);
        let legs = fixed_rate_bond(Currency::USD, 1_000_000 * RAY, bps_to_ray(300), start, mat, Frequency::Annual);
        let cfs = build_trade(&legs);
        assert_eq!(cfs.len(), 6); // 5 coupons + 1 principal
        assert_eq!(cfs[5].cf_type, CashflowType::NotionalExchange);
    }

    #[test]
    fn test_xccy_has_notional_exchanges() {
        let start = ymd_to_unix(2026, 3, 20);
        let end = ymd_to_unix(2029, 3, 20);
        let legs = xccy_swap(Currency::USD, Currency::EUR,
            10_000_000 * RAY, 9_000_000 * RAY, bps_to_ray(300), start, end);
        let cfs = build_trade(&legs);
        let exchanges: Vec<_> = cfs.iter().filter(|cf| cf.cf_type == CashflowType::NotionalExchange).collect();
        assert_eq!(exchanges.len(), 4); // 2 initial + 2 final
    }

    #[test]
    fn test_amortizing_decreasing_notional() {
        let start = ymd_to_unix(2026, 3, 20);
        let end = ymd_to_unix(2031, 3, 20);
        let sched = vec![
            10_000_000 * RAY, 8_000_000 * RAY, 6_000_000 * RAY,
            4_000_000 * RAY, 2_000_000 * RAY,
        ];
        let legs = amortizing_irs(Currency::GBP, sched, bps_to_ray(300), start, end, true);
        let cfs = build_trade(&legs);
        // Fixed leg period 5 should use 2M notional
        let fixed_cfs: Vec<_> = cfs.iter().filter(|cf| cf.cf_type == CashflowType::Fixed).collect();
        assert_eq!(fixed_cfs[4].notional, 2_000_000 * RAY);
    }

    #[test]
    fn test_irs_at_par_via_template() {
        let start = ymd_to_unix(2026, 3, 20);
        let end = ymd_to_unix(2031, 3, 20);
        let rate = bps_to_ray(400);
        let legs = irs(Currency::GBP, 10_000_000 * RAY, rate, start, end, true);
        let cfs = build_trade(&legs);

        let rates = vec![rate; 5];
        let dfs = bootstrap_annual(&rates).unwrap();
        let dates: Vec<u64> = (1..=5).map(|i| start + i * 365 * 86400).collect();

        let pv = value_cashflows(&cfs, start, &dates, &dfs);
        let pv_f64 = pv as f64 / RAY as f64;
        // Tolerance is larger because schedule-generated dates may differ slightly
        // from the hardcoded dates used for bootstrapping
        assert!(pv_f64.abs() < 2000.0, "IRS at par = {}", pv_f64);
    }

    #[test]
    fn test_bond_pv_near_par() {
        let start = ymd_to_unix(2026, 3, 20);
        let mat = ymd_to_unix(2031, 3, 20);
        let rate = bps_to_ray(400);
        let legs = fixed_rate_bond(Currency::GBP, 1_000_000 * RAY, rate, start, mat, Frequency::Annual);
        let cfs = build_trade(&legs);

        let rates = vec![rate; 5];
        let dfs = bootstrap_annual(&rates).unwrap();
        let dates: Vec<u64> = (1..=5).map(|i| start + i * 365 * 86400).collect();

        let pv = value_cashflows(&cfs, start, &dates, &dfs);
        let pv_f64 = pv as f64 / RAY as f64;
        // Bond at par rate should be worth ~face value
        assert!((pv_f64 - 1_000_000.0).abs() < 100.0, "Bond PV = {}", pv_f64);
    }
}
