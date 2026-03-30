/// Schedule generation per SPEC-005 §3.
/// Generates payment dates by stepping backward from end date.

use crate::daycount::unix_to_ymd;

const SECONDS_PER_DAY: u64 = 86400;

/// Payment frequency.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum Frequency {
    ZeroCoupon = 0,
    Annual = 1,
    SemiAnnual = 2,
    Quarterly = 4,
    Monthly = 12,
}

impl Frequency {
    /// Months per period.
    pub fn months(self) -> i32 {
        match self {
            Frequency::ZeroCoupon => 0,
            Frequency::Annual => 12,
            Frequency::SemiAnnual => 6,
            Frequency::Quarterly => 3,
            Frequency::Monthly => 1,
        }
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Frequency::ZeroCoupon),
            1 => Some(Frequency::Annual),
            2 => Some(Frequency::SemiAnnual),
            4 => Some(Frequency::Quarterly),
            12 => Some(Frequency::Monthly),
            _ => None,
        }
    }
}

/// Generate payment dates by rolling backward from end to start.
///
/// Returns timestamps in ascending order. The first accrual period
/// starts at `start`, the last payment is at `end`.
pub fn generate_schedule(start: u64, end: u64, frequency: Frequency) -> Vec<u64> {
    if frequency == Frequency::ZeroCoupon {
        return vec![end];
    }

    let months_per_period = frequency.months();
    let (end_y, end_m, end_d) = unix_to_ymd(end);

    // Roll backward from end date
    let mut dates: Vec<u64> = Vec::new();
    let mut i = 0i32;
    loop {
        let total_months_back = (i + 1) * months_per_period;
        let mut y = end_y;
        let mut m = end_m - total_months_back;

        // Normalize month/year
        while m <= 0 {
            m += 12;
            y -= 1;
        }
        while m > 12 {
            m -= 12;
            y += 1;
        }

        // Clamp day to valid range for the month
        let d = end_d.min(days_in_month(y, m));

        let ts = ymd_to_unix(y, m, d);
        if ts <= start {
            break;
        }
        dates.push(ts);
        i += 1;
    }

    // Reverse to ascending order, then add end date
    dates.reverse();
    dates.push(ymd_to_unix(end_y, end_m, end_d));
    dates
}

/// Convert (year, month, day) to Unix timestamp (midnight UTC).
/// Inverse of unix_to_ymd.
pub fn ymd_to_unix(y: i32, m: i32, d: i32) -> u64 {
    // Howard Hinnant's days_from_civil algorithm
    let y = if m <= 2 { y as i64 - 1 } else { y as i64 };
    let m = m as i64;
    let d = d as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy as u64;
    let days = era as i64 * 146097 + doe as i64 - 719468;
    (days as u64) * SECONDS_PER_DAY
}

/// Days in a given month (handles leap years).
fn days_in_month(y: i32, m: i32) -> i32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daycount::unix_to_ymd;

    #[test]
    fn test_ymd_roundtrip() {
        let ts = 1_773_964_800u64; // 2026-03-20
        let (y, m, d) = unix_to_ymd(ts);
        assert_eq!((y, m, d), (2026, 3, 20));
        let ts2 = ymd_to_unix(y, m, d);
        assert_eq!(ts, ts2);
    }

    #[test]
    fn test_ymd_roundtrip_leap() {
        let ts = ymd_to_unix(2024, 2, 29);
        let (y, m, d) = unix_to_ymd(ts);
        assert_eq!((y, m, d), (2024, 2, 29));
    }

    #[test]
    fn test_annual_schedule() {
        let start = ymd_to_unix(2026, 3, 20);
        let end = ymd_to_unix(2031, 3, 20);
        let dates = generate_schedule(start, end, Frequency::Annual);
        assert_eq!(dates.len(), 5);
        // Check all dates are March 20
        for &d in &dates {
            let (_, m, day) = unix_to_ymd(d);
            assert_eq!((m, day), (3, 20));
        }
    }

    #[test]
    fn test_semi_annual_schedule() {
        let start = ymd_to_unix(2026, 3, 20);
        let end = ymd_to_unix(2028, 3, 20);
        let dates = generate_schedule(start, end, Frequency::SemiAnnual);
        assert_eq!(dates.len(), 4); // 4 semi-annual periods in 2 years
    }

    #[test]
    fn test_quarterly_schedule() {
        let start = ymd_to_unix(2026, 3, 20);
        let end = ymd_to_unix(2027, 3, 20);
        let dates = generate_schedule(start, end, Frequency::Quarterly);
        assert_eq!(dates.len(), 4);
    }

    #[test]
    fn test_monthly_schedule() {
        let start = ymd_to_unix(2026, 1, 1);
        let end = ymd_to_unix(2027, 1, 1);
        let dates = generate_schedule(start, end, Frequency::Monthly);
        assert_eq!(dates.len(), 12);
    }

    #[test]
    fn test_zero_coupon() {
        let start = ymd_to_unix(2026, 3, 20);
        let end = ymd_to_unix(2031, 3, 20);
        let dates = generate_schedule(start, end, Frequency::ZeroCoupon);
        assert_eq!(dates.len(), 1);
        assert_eq!(dates[0], end);
    }

    #[test]
    fn test_month_end_roll() {
        // Start Jan 31 → Feb should be 28 (non-leap)
        let start = ymd_to_unix(2026, 1, 31);
        let end = ymd_to_unix(2026, 7, 31);
        let dates = generate_schedule(start, end, Frequency::Monthly);
        let (_, m2, d2) = unix_to_ymd(dates[0]); // Feb
        assert_eq!((m2, d2), (2, 28));
    }

    #[test]
    fn test_ascending_order() {
        let start = ymd_to_unix(2026, 3, 20);
        let end = ymd_to_unix(2031, 3, 20);
        let dates = generate_schedule(start, end, Frequency::SemiAnnual);
        for i in 1..dates.len() {
            assert!(dates[i] > dates[i - 1], "Dates must be ascending");
        }
    }
}
