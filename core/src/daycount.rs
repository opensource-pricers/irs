/// Day count year fraction computation per SPEC-003 §4.
///
/// All timestamps are Unix time in seconds (UTC).
/// Returns year fraction in RAY precision.

use crate::math::{Ray, RAY};
use crate::conventions::DayCountConvention;

const SECONDS_PER_DAY: u128 = 86_400;

/// Compute year fraction between two timestamps.
///
/// # Panics
/// Panics if t_end < t_start.
pub fn year_fraction(convention: DayCountConvention, t_start: u64, t_end: u64) -> Ray {
    assert!(t_end >= t_start, "t_end must be >= t_start");
    let elapsed = (t_end - t_start) as u128;

    match convention {
        DayCountConvention::Act360 => {
            // τ = elapsed_seconds / (360 * 86400)
            (elapsed * RAY) / (360 * SECONDS_PER_DAY)
        }
        DayCountConvention::Act365Fixed => {
            // τ = elapsed_seconds / (365 * 86400)
            (elapsed * RAY) / (365 * SECONDS_PER_DAY)
        }
        DayCountConvention::Thirty360BondBasis => {
            // Requires calendar date decomposition
            let (y1, m1, d1) = unix_to_ymd(t_start);
            let (y2, m2, d2) = unix_to_ymd(t_end);
            thirty_360_bond_basis(y1, m1, d1, y2, m2, d2)
        }
        DayCountConvention::ThirtyE360 => {
            let (y1, m1, d1) = unix_to_ymd(t_start);
            let (y2, m2, d2) = unix_to_ymd(t_end);
            thirty_e360(y1, m1, d1, y2, m2, d2)
        }
    }
}

/// Compute year fractions for a schedule of payment dates.
pub fn year_fractions(
    convention: DayCountConvention,
    settlement: u64,
    payment_dates: &[u64],
) -> Vec<Ray> {
    let mut taus = Vec::with_capacity(payment_dates.len());
    let mut prev = settlement;
    for &date in payment_dates {
        taus.push(year_fraction(convention, prev, date));
        prev = date;
    }
    taus
}

// --- 30/360 implementations ---

/// 30/360 Bond Basis (ISDA 2006).
/// days = 360*(Y2-Y1) + 30*(M2-M1) + (D2'-D1')
/// where D1' = min(D1, 30); if D1'=30 then D2' = min(D2, 30)
fn thirty_360_bond_basis(y1: i32, m1: i32, d1: i32, y2: i32, m2: i32, d2: i32) -> Ray {
    let d1_adj = d1.min(30);
    let d2_adj = if d1_adj == 30 { d2.min(30) } else { d2 };
    let days = 360 * (y2 - y1) + 30 * (m2 - m1) + (d2_adj - d1_adj);
    (days.unsigned_abs() as u128 * RAY) / 360
}

/// 30E/360.
/// D1' = min(D1, 30); D2' = min(D2, 30)
fn thirty_e360(y1: i32, m1: i32, d1: i32, y2: i32, m2: i32, d2: i32) -> Ray {
    let d1_adj = d1.min(30);
    let d2_adj = d2.min(30);
    let days = 360 * (y2 - y1) + 30 * (m2 - m1) + (d2_adj - d1_adj);
    (days.unsigned_abs() as u128 * RAY) / 360
}

/// Convert Unix timestamp to (year, month, day) in UTC.
/// Civil calendar computation (proleptic Gregorian).
pub fn unix_to_ymd(ts: u64) -> (i32, i32, i32) {
    // Algorithm from Howard Hinnant's civil_from_days
    let z = (ts / SECONDS_PER_DAY as u64) as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64; // day of era
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as i32, d as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::ray_to_f64;

    const DAY: u64 = 86400;
    const SETTLE: u64 = 1_773_964_800; // 2026-03-20 00:00 UTC

    #[test]
    fn test_act360_360_days() {
        let tau = year_fraction(DayCountConvention::Act360, SETTLE, SETTLE + 360 * DAY);
        assert_eq!(tau, RAY); // exactly 1.0
    }

    #[test]
    fn test_act365_365_days() {
        let tau = year_fraction(DayCountConvention::Act365Fixed, SETTLE, SETTLE + 365 * DAY);
        assert_eq!(tau, RAY); // exactly 1.0
    }

    #[test]
    fn test_act360_365_days() {
        let tau = year_fraction(DayCountConvention::Act360, SETTLE, SETTLE + 365 * DAY);
        let expected = (365u128 * RAY) / 360;
        assert_eq!(tau, expected); // 365/360 > 1
    }

    #[test]
    fn test_act360_half_year() {
        let tau = year_fraction(DayCountConvention::Act360, SETTLE, SETTLE + 180 * DAY);
        assert_eq!(tau, RAY / 2); // 0.5
    }

    #[test]
    fn test_unix_to_ymd() {
        // 2026-03-20 00:00 UTC
        let (y, m, d) = unix_to_ymd(SETTLE);
        assert_eq!((y, m, d), (2026, 3, 20));
    }

    #[test]
    fn test_unix_to_ymd_leap() {
        // 2024-02-29 (leap day)
        let ts = 1_709_164_800; // 2024-02-29 00:00 UTC
        let (y, m, d) = unix_to_ymd(ts);
        assert_eq!((y, m, d), (2024, 2, 29));
    }

    #[test]
    fn test_thirty_360() {
        // 2026-03-20 to 2027-03-20 = 360 days in 30/360 = 1.0
        let tau = year_fraction(
            DayCountConvention::Thirty360BondBasis,
            SETTLE,
            SETTLE + 365 * DAY,
        );
        let val = ray_to_f64(tau);
        assert!((val - 1.0).abs() < 0.01, "30/360 1Y should be ~1.0, got {}", val);
    }

    #[test]
    fn test_year_fractions_schedule() {
        let dates = vec![
            SETTLE + 365 * DAY,
            SETTLE + 730 * DAY,
            SETTLE + 1095 * DAY,
        ];
        let taus = year_fractions(DayCountConvention::Act365Fixed, SETTLE, &dates);
        assert_eq!(taus.len(), 3);
        assert_eq!(taus[0], RAY);
        assert_eq!(taus[1], RAY);
        assert_eq!(taus[2], RAY);
    }
}
