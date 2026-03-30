/// RAY fixed-point arithmetic — 27 decimal places using U256.
///
/// 1 RAY = 10^27. A discount factor of 0.95 = 950_000_000_000_000_000_000_000_000.
/// Uses ethnum::U256 for overflow-safe intermediate products.
use ethnum::U256;

/// 10^27
pub const RAY: u128 = 1_000_000_000_000_000_000_000_000_000;
/// 5 * 10^26 (for half-up rounding)
pub const HALF_RAY: u128 = 500_000_000_000_000_000_000_000_000;
/// 1 basis point in RAY = 0.0001 * 10^27 = 10^23
pub const ONE_BP: u128 = 100_000_000_000_000_000_000_000;

/// A RAY-precision unsigned value.
pub type Ray = u128;

/// Multiply two RAY numbers: (a * b + HALF_RAY) / RAY
///
/// Uses U256 intermediate to prevent overflow.
/// Max safe inputs: a, b < 2^128 (u128::MAX ≈ 3.4e38).
/// Product a*b < 2^256, always fits U256.
#[inline]
pub fn ray_mul(a: Ray, b: Ray) -> Ray {
    let a256 = U256::from(a);
    let b256 = U256::from(b);
    let ray256 = U256::from(RAY);
    let half256 = U256::from(HALF_RAY);
    let result = (a256 * b256 + half256) / ray256;
    result.as_u128()
}

/// Divide two RAY numbers: (a * RAY + b/2) / b
///
/// Panics if b == 0.
#[inline]
pub fn ray_div(a: Ray, b: Ray) -> Ray {
    assert!(b > 0, "division by zero");
    let a256 = U256::from(a);
    let b256 = U256::from(b);
    let ray256 = U256::from(RAY);
    let result = (a256 * ray256 + b256 / 2) / b256;
    result.as_u128()
}

/// Non-panicking version of ray_div. Returns None if b == 0.
#[inline]
pub fn ray_div_checked(a: Ray, b: Ray) -> Option<Ray> {
    if b == 0 { return None; }
    let a256 = U256::from(a);
    let b256 = U256::from(b);
    let ray256 = U256::from(RAY);
    let result = (a256 * ray256 + b256 / 2) / b256;
    Some(result.as_u128())
}

/// Convert basis points to RAY: 100bp = 1% = 0.01 * RAY = 10^25
#[inline]
pub fn bps_to_ray(bps: u32) -> Ray {
    (bps as u128) * ONE_BP
}

/// Convert RAY to f64 for display/comparison.
#[inline]
pub fn ray_to_f64(r: Ray) -> f64 {
    r as f64 / RAY as f64
}

/// Convert f64 to RAY.
#[inline]
pub fn f64_to_ray(v: f64) -> Ray {
    let result = v * RAY as f64;
    if result < 0.0 || result > u128::MAX as f64 {
        0
    } else {
        result as u128
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ray_mul_identity() {
        assert_eq!(ray_mul(RAY, RAY), RAY); // 1 * 1 = 1
    }

    #[test]
    fn test_ray_mul_half() {
        let half = RAY / 2;
        assert_eq!(ray_mul(half, half), RAY / 4); // 0.5 * 0.5 = 0.25
    }

    #[test]
    fn test_ray_div_identity() {
        assert_eq!(ray_div(RAY, RAY), RAY); // 1 / 1 = 1
    }

    #[test]
    fn test_ray_div_half() {
        assert_eq!(ray_div(RAY, 2 * RAY), RAY / 2); // 1 / 2 = 0.5
    }

    #[test]
    fn test_bps_to_ray() {
        let five_pct = bps_to_ray(500);
        assert_eq!(five_pct, 50_000_000_000_000_000_000_000_000); // 0.05 * RAY
    }

    #[test]
    fn test_ray_mul_large() {
        // 0.95 * 0.95 = 0.9025
        let a = 950_000_000_000_000_000_000_000_000u128;
        let result = ray_mul(a, a);
        let expected = 902_500_000_000_000_000_000_000_000u128;
        assert!((result as i128 - expected as i128).unsigned_abs() <= 1);
    }

    #[test]
    fn test_roundtrip() {
        let val = 0.97531f64;
        let ray = f64_to_ray(val);
        let back = ray_to_f64(ray);
        assert!((val - back).abs() < 1e-15);
    }
}
