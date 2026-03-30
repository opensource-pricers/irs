/// Overnight rate fixing store and compounding engine.
///
/// For OIS floating legs, the coupon is NOT a forward rate.
/// It's the compounded realized overnight rate over the accrual period:
///
///   CompoundedRate = Π(1 + r_i × n_i/N) - 1
///
/// where r_i = overnight rate on day i
///       n_i = number of calendar days that fixing applies (1, or 3 over weekends)
///       N   = 360 (ACT/360) or 365 (ACT/365F)

use crate::math::{Ray, RAY, ray_to_f64};

/// A single daily overnight rate fixing.
#[derive(Debug, Clone)]
pub struct Fixing {
    /// Date as YYYYMMDD integer.
    pub date: u32,
    /// Overnight rate in percent (e.g., 3.62 = 3.62%).
    pub rate_pct: f64,
    /// Number of calendar days this fixing covers (1 for weekdays, 3 for Friday).
    pub days: u32,
}

/// Compute the compounded overnight rate over a period.
///
/// Formula (ACT/360):
///   CompoundedRate = (Π(1 + r_i/100 × days_i/360) - 1) × 360/total_days
///
/// Returns the annualized compounded rate as a percentage.
pub fn compound_overnight_rate(fixings: &[Fixing], day_basis: u32) -> f64 {
    if fixings.is_empty() {
        return 0.0;
    }

    let mut product = 1.0f64;
    let mut total_days: u32 = 0;

    for f in fixings {
        let daily_factor = 1.0 + f.rate_pct / 100.0 * f.days as f64 / day_basis as f64;
        product *= daily_factor;
        total_days += f.days;
    }

    // Annualized compounded rate
    (product - 1.0) * day_basis as f64 / total_days as f64 * 100.0
}

/// Compute the floating coupon amount for one period.
///
/// coupon = notional × compounded_rate/100 × total_days/day_basis
pub fn floating_coupon(
    notional: f64,
    fixings: &[Fixing],
    day_basis: u32,
) -> f64 {
    if fixings.is_empty() {
        return 0.0;
    }

    let mut product = 1.0f64;
    for f in fixings {
        product *= 1.0 + f.rate_pct / 100.0 * f.days as f64 / day_basis as f64;
    }

    notional * (product - 1.0)
}

/// A fixing store — holds daily overnight rates indexed by date.
#[derive(Debug, Clone, Default)]
pub struct FixingStore {
    /// SOFR daily fixings, sorted by date ascending.
    pub sofr: Vec<Fixing>,
    /// ESTR daily fixings, sorted by date ascending.
    pub estr: Vec<Fixing>,
}

impl FixingStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a SOFR fixing.
    pub fn add_sofr(&mut self, date: u32, rate_pct: f64, days: u32) {
        self.sofr.push(Fixing { date, rate_pct, days });
        self.sofr.sort_by_key(|f| f.date);
    }

    /// Add an ESTR fixing.
    pub fn add_estr(&mut self, date: u32, rate_pct: f64, days: u32) {
        self.estr.push(Fixing { date, rate_pct, days });
        self.estr.sort_by_key(|f| f.date);
    }

    /// Get fixings for a date range [start_date, end_date).
    pub fn get_fixings(&self, currency: &str, start_date: u32, end_date: u32) -> Vec<&Fixing> {
        let store = match currency {
            "USD" | "SOFR" => &self.sofr,
            "EUR" | "ESTR" => &self.estr,
            _ => return vec![],
        };
        store.iter()
            .filter(|f| f.date >= start_date && f.date < end_date)
            .collect()
    }

    /// Compute compounded rate for a period.
    pub fn compounded_rate(&self, currency: &str, start_date: u32, end_date: u32) -> f64 {
        let fixings: Vec<Fixing> = self.get_fixings(currency, start_date, end_date)
            .into_iter().cloned().collect();
        let basis = match currency {
            "USD" | "SOFR" => 360,
            _ => 360, // ESTR also uses ACT/360
        };
        compound_overnight_rate(&fixings, basis)
    }

    /// Compute floating coupon for a period.
    pub fn period_coupon(&self, currency: &str, notional: f64, start_date: u32, end_date: u32) -> f64 {
        let fixings: Vec<Fixing> = self.get_fixings(currency, start_date, end_date)
            .into_iter().cloned().collect();
        let basis = match currency {
            "USD" | "SOFR" => 360,
            _ => 360,
        };
        floating_coupon(notional, &fixings, basis)
    }

    pub fn sofr_count(&self) -> usize { self.sofr.len() }
    pub fn estr_count(&self) -> usize { self.estr.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compound_single_day() {
        // One day at 3.65%, ACT/360
        let fixings = vec![Fixing { date: 20260310, rate_pct: 3.65, days: 1 }];
        let rate = compound_overnight_rate(&fixings, 360);
        // Should be approximately 3.65% annualized
        assert!((rate - 3.65).abs() < 0.01, "rate = {}", rate);
    }

    #[test]
    fn test_compound_weekend() {
        // Friday fixing covers 3 days (Fri-Sat-Sun)
        let fixings = vec![
            Fixing { date: 20260306, rate_pct: 3.65, days: 3 }, // Friday
            Fixing { date: 20260309, rate_pct: 3.64, days: 1 }, // Monday
            Fixing { date: 20260310, rate_pct: 3.64, days: 1 }, // Tuesday
        ];
        let rate = compound_overnight_rate(&fixings, 360);
        // Should be close to ~3.645%
        assert!((rate - 3.645).abs() < 0.05, "rate = {}", rate);
    }

    #[test]
    fn test_floating_coupon() {
        // 10M notional, 5 business days at 3.65%, ACT/360
        // Expected: 10M × 3.65% × 7/360 ≈ 7,097
        let fixings = vec![
            Fixing { date: 20260306, rate_pct: 3.65, days: 3 }, // Fri (covers 3 days)
            Fixing { date: 20260309, rate_pct: 3.65, days: 1 },
            Fixing { date: 20260310, rate_pct: 3.65, days: 1 },
            Fixing { date: 20260311, rate_pct: 3.65, days: 1 },
            Fixing { date: 20260312, rate_pct: 3.65, days: 1 },
        ];
        let coupon = floating_coupon(10_000_000.0, &fixings, 360);
        let expected = 10_000_000.0 * 0.0365 * 7.0 / 360.0;
        assert!((coupon - expected).abs() < 10.0, "coupon = {}, expected = {}", coupon, expected);
    }

    #[test]
    fn test_fixing_store() {
        let mut store = FixingStore::new();
        store.add_sofr(20260306, 3.65, 3);
        store.add_sofr(20260309, 3.64, 1);
        store.add_sofr(20260310, 3.64, 1);

        assert_eq!(store.sofr_count(), 3);

        let rate = store.compounded_rate("USD", 20260306, 20260311);
        assert!(rate > 3.0 && rate < 4.0, "rate = {}", rate);

        let coupon = store.period_coupon("USD", 50_000_000.0, 20260306, 20260311);
        assert!(coupon > 0.0, "coupon = {}", coupon);
    }
}
