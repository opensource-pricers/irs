/// Linear interpolation on discount factors per SPEC-003 §5.
use crate::math::{Ray, RAY, ray_mul};

/// Interpolate a discount factor at time `t` from a curve of (dates, dfs).
///
/// Uses linear interpolation on discount factors between nodes:
///   DF(t) = (1-w) * DF[j] + w * DF[j+1]
///
/// # Arguments
/// * `t` — target Unix timestamp
/// * `settlement` — curve settlement timestamp (DF=1 at this point)
/// * `dates` — curve node timestamps, ascending
/// * `dfs` — discount factors at each node, in RAY
pub fn interpolate_df(t: u64, settlement: u64, dates: &[u64], dfs: &[Ray]) -> Ray {
    assert_eq!(dates.len(), dfs.len());

    if t <= settlement {
        return RAY;
    }

    // Before first node: linear in DF from 1.0 at settlement
    if t <= dates[0] {
        let w_num = (t - settlement) as u128;
        let w_den = (dates[0] - settlement) as u128;
        if w_den == 0 {
            return dfs[0];
        }
        let w = (w_num * RAY) / w_den;
        // DF(t) = 1 - w * (1 - DF[0])
        return RAY - ray_mul(w, RAY - dfs[0]);
    }

    // Between nodes: linear interpolation
    let n = dates.len();
    for j in 0..n - 1 {
        if t <= dates[j + 1] {
            let span = (dates[j + 1] - dates[j]) as u128;
            if span == 0 {
                return dfs[j];
            }
            let w = ((t - dates[j]) as u128 * RAY) / span;
            // DF = DF[j] * (1-w) + DF[j+1] * w
            return ray_mul(RAY - w, dfs[j]) + ray_mul(w, dfs[j + 1]);
        }
    }

    // Beyond last node: flat forward extrapolation
    dfs[n - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: u64 = 86400;
    const SETTLE: u64 = 1_773_964_800;

    #[test]
    fn test_at_settlement() {
        let dates = vec![SETTLE + 365 * DAY];
        let dfs = vec![RAY * 95 / 100]; // 0.95
        assert_eq!(interpolate_df(SETTLE, SETTLE, &dates, &dfs), RAY);
    }

    #[test]
    fn test_at_node() {
        let dates = vec![SETTLE + 365 * DAY, SETTLE + 730 * DAY];
        let dfs = vec![RAY * 95 / 100, RAY * 90 / 100];
        assert_eq!(interpolate_df(dates[0], SETTLE, &dates, &dfs), dfs[0]);
        assert_eq!(interpolate_df(dates[1], SETTLE, &dates, &dfs), dfs[1]);
    }

    #[test]
    fn test_midpoint() {
        let dates = vec![SETTLE + 365 * DAY, SETTLE + 730 * DAY];
        let dfs = vec![RAY * 96 / 100, RAY * 92 / 100]; // 0.96, 0.92
        let mid = SETTLE + 547 * DAY; // ~1.5 years
        let df_mid = interpolate_df(mid, SETTLE, &dates, &dfs);
        // Should be between 0.92 and 0.96
        assert!(df_mid > dfs[1] && df_mid < dfs[0]);
    }

    #[test]
    fn test_before_first_node() {
        let dates = vec![SETTLE + 365 * DAY];
        let dfs = vec![RAY * 96 / 100];
        let t = SETTLE + 182 * DAY; // ~6 months
        let df = interpolate_df(t, SETTLE, &dates, &dfs);
        // Should be between 0.96 and 1.0
        assert!(df > dfs[0] && df < RAY);
    }

    #[test]
    fn test_beyond_last_node() {
        let dates = vec![SETTLE + 365 * DAY];
        let dfs = vec![RAY * 96 / 100];
        let t = SETTLE + 730 * DAY; // 2 years, beyond curve
        let df = interpolate_df(t, SETTLE, &dates, &dfs);
        assert_eq!(df, dfs[0]); // flat extrapolation
    }
}
