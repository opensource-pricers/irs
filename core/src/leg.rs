/// Leg builder per SPEC-005 §3.
/// Assembles cashflows from a leg descriptor.

use crate::math::{Ray, RAY, ray_mul};
use crate::conventions::{Currency, DayCountConvention};
use crate::daycount::year_fraction;
use crate::schedule::{Frequency, generate_schedule};
use crate::cashflow::{Cashflow, CashflowType};

/// Leg descriptor — defines one side of a trade.
#[derive(Debug, Clone)]
pub struct LegDescriptor {
    pub leg_type: LegType,
    pub direction: i8,              // +1 receive, -1 pay
    pub currency: Currency,
    pub day_count: DayCountConvention,
    pub notional: Ray,
    pub start_date: u64,
    pub end_date: u64,
    pub frequency: Frequency,
    pub fixed_rate: Ray,            // for Fixed legs
    pub spread: i128,               // for Floating legs (RAY-scaled)
    pub notional_schedule: Option<Vec<Ray>>,  // amortizing
    pub rate_schedule: Option<Vec<Ray>>,      // step-up/step-down
    pub exchange_notional_start: bool,        // XCCY: exchange at start
    pub exchange_notional_end: bool,          // XCCY: exchange at maturity
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LegType {
    Fixed,
    Floating,
}

impl LegDescriptor {
    /// Build a simple fixed leg.
    pub fn fixed(currency: Currency, notional: Ray, rate: Ray, start: u64, end: u64, freq: Frequency, direction: i8) -> Self {
        Self {
            leg_type: LegType::Fixed, direction, currency,
            day_count: currency.convention().day_count,
            notional, start_date: start, end_date: end, frequency: freq,
            fixed_rate: rate, spread: 0,
            notional_schedule: None, rate_schedule: None,
            exchange_notional_start: false, exchange_notional_end: false,
        }
    }

    /// Build a simple floating leg.
    pub fn floating(currency: Currency, notional: Ray, spread: i128, start: u64, end: u64, freq: Frequency, direction: i8) -> Self {
        Self {
            leg_type: LegType::Floating, direction, currency,
            day_count: currency.convention().day_count,
            notional, start_date: start, end_date: end, frequency: freq,
            fixed_rate: 0, spread,
            notional_schedule: None, rate_schedule: None,
            exchange_notional_start: false, exchange_notional_end: false,
        }
    }
}

/// Build cashflows from a leg descriptor.
pub fn build_leg(desc: &LegDescriptor) -> Vec<Cashflow> {
    let dates = generate_schedule(desc.start_date, desc.end_date, desc.frequency);
    let n = dates.len();
    let mut cashflows = Vec::with_capacity(n + 2); // + possible notional exchanges

    // Notional exchange at start (XCCY)
    if desc.exchange_notional_start {
        cashflows.push(Cashflow {
            cf_type: CashflowType::NotionalExchange,
            direction: desc.direction,
            payment_date: desc.start_date,
            notional: desc.notional,
            fixed_amount: 0,
            accrual_start: 0,
            accrual_end: 0,
            spread: 0,
            condition_met: false,
        });
    }

    for i in 0..n {
        let accrual_start = if i == 0 { desc.start_date } else { dates[i - 1] };
        let accrual_end = dates[i];
        let payment_date = dates[i];

        let period_notional = match &desc.notional_schedule {
            Some(sched) if i < sched.len() => sched[i],
            _ => desc.notional,
        };

        match desc.leg_type {
            LegType::Fixed => {
                let rate = match &desc.rate_schedule {
                    Some(sched) if i < sched.len() => sched[i],
                    _ => desc.fixed_rate,
                };
                let tau = year_fraction(desc.day_count, accrual_start, accrual_end);
                let amount = ray_mul(ray_mul(period_notional, rate), tau);
                let signed_amount = desc.direction as i128 * amount as i128;

                cashflows.push(Cashflow {
                    cf_type: CashflowType::Fixed,
                    direction: desc.direction,
                    payment_date,
                    notional: period_notional,
                    fixed_amount: signed_amount,
                    accrual_start,
                    accrual_end,
                    spread: 0,
                    condition_met: false,
                });
            }
            LegType::Floating => {
                cashflows.push(Cashflow {
                    cf_type: CashflowType::Floating,
                    direction: desc.direction,
                    payment_date,
                    notional: period_notional,
                    fixed_amount: 0,
                    accrual_start,
                    accrual_end,
                    spread: desc.spread,
                    condition_met: false,
                });
            }
        }
    }

    // Notional exchange at end (XCCY, bonds)
    if desc.exchange_notional_end {
        cashflows.push(Cashflow {
            cf_type: CashflowType::NotionalExchange,
            direction: desc.direction,
            payment_date: desc.end_date,
            notional: desc.notional,
            fixed_amount: 0,
            accrual_start: 0,
            accrual_end: 0,
            spread: 0,
            condition_met: false,
        });
    }

    cashflows
}

/// Build cashflows for all legs in a trade.
pub fn build_trade(legs: &[LegDescriptor]) -> Vec<Cashflow> {
    legs.iter().flat_map(build_leg).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{bps_to_ray, ray_to_f64};
    use crate::bootstrap::bootstrap_annual;
    use crate::cashflow::value_cashflows;

    const SETTLE: u64 = 1_773_964_800; // 2026-03-20
    const DAY: u64 = 86400;

    #[test]
    fn test_build_fixed_leg() {
        let leg = LegDescriptor::fixed(
            Currency::GBP, 10_000_000 * RAY, bps_to_ray(400),
            SETTLE, SETTLE + 5 * 365 * DAY, Frequency::Annual, -1,
        );
        let cfs = build_leg(&leg);
        assert_eq!(cfs.len(), 5);
        assert!(cfs.iter().all(|cf| cf.cf_type == CashflowType::Fixed));
        assert!(cfs.iter().all(|cf| cf.direction == -1));
    }

    #[test]
    fn test_build_floating_leg() {
        let leg = LegDescriptor::floating(
            Currency::GBP, 10_000_000 * RAY, 0,
            SETTLE, SETTLE + 5 * 365 * DAY, Frequency::SemiAnnual, 1,
        );
        let cfs = build_leg(&leg);
        assert_eq!(cfs.len(), 10); // 5Y × 2 per year
        assert!(cfs.iter().all(|cf| cf.cf_type == CashflowType::Floating));
    }

    #[test]
    fn test_irs_at_par() {
        let notional = 10_000_000 * RAY;
        let rate = bps_to_ray(400);
        let end = SETTLE + 5 * 365 * DAY;

        let legs = vec![
            LegDescriptor::fixed(Currency::GBP, notional, rate, SETTLE, end, Frequency::Annual, -1),
            LegDescriptor::floating(Currency::GBP, notional, 0, SETTLE, end, Frequency::Annual, 1),
        ];

        let cfs = build_trade(&legs);
        let rates = vec![rate; 5];
        let dfs = bootstrap_annual(&rates).unwrap();
        let dates: Vec<u64> = (1..=5).map(|i| SETTLE + i * 365 * DAY).collect();

        let pv = value_cashflows(&cfs, SETTLE, &dates, &dfs);
        let pv_f64 = pv as f64 / RAY as f64;
        assert!(pv_f64.abs() < 100.0, "IRS at par should have PV~0, got {}", pv_f64);
    }

    #[test]
    fn test_amortizing_leg() {
        let sched = vec![
            10_000_000 * RAY,
            8_000_000 * RAY,
            6_000_000 * RAY,
            4_000_000 * RAY,
            2_000_000 * RAY,
        ];
        let mut leg = LegDescriptor::fixed(
            Currency::GBP, 10_000_000 * RAY, bps_to_ray(300),
            SETTLE, SETTLE + 5 * 365 * DAY, Frequency::Annual, -1,
        );
        leg.notional_schedule = Some(sched);
        let cfs = build_leg(&leg);
        assert_eq!(cfs.len(), 5);
        // Last period notional should be 2M
        assert_eq!(cfs[4].notional, 2_000_000 * RAY);
    }

    #[test]
    fn test_notional_exchange() {
        let mut leg = LegDescriptor::fixed(
            Currency::USD, 50_000_000 * RAY, bps_to_ray(300),
            SETTLE, SETTLE + 3 * 365 * DAY, Frequency::Annual, 1,
        );
        leg.exchange_notional_end = true;
        let cfs = build_leg(&leg);
        // 3 fixed coupons + 1 notional exchange
        assert_eq!(cfs.len(), 4);
        assert_eq!(cfs[3].cf_type, CashflowType::NotionalExchange);
    }
}
