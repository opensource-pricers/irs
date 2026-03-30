/// Settlement instruction generation per SPEC-011.
///
/// Computes what to pay, to whom, when — but does NOT move money.
/// Output is consumed by SWIFT, Fnality, Partior, or internal ledger.

use crate::math::{Ray, RAY, ray_mul, ray_to_f64};
use crate::bootstrap::{bootstrap_annual, forward_rate};
use crate::conventions::Currency;
use crate::daycount::year_fraction;

/// A single settlement instruction between two parties.
#[derive(Debug, Clone)]
pub struct SettlementInstruction {
    pub payment_date: u64,
    pub payer: usize,        // bank index
    pub receiver: usize,     // bank index
    pub currency: Currency,
    pub net_amount: f64,     // positive = payer pays receiver
    pub fixed_gross: f64,
    pub floating_gross: f64,
    pub swap_refs: Vec<usize>,
}

/// A bilateral swap's coupon payment on a specific date.
#[derive(Debug)]
struct CouponPayment {
    pub swap_ref: usize,
    pub payer: usize,
    pub receiver: usize,
    pub fixed_amount: f64,
    pub floating_amount: f64,
    pub net: f64, // positive = payer owes receiver
}

/// Generate settlement instructions for a set of swaps on a given payment date.
///
/// For each swap with a cashflow on `payment_date`:
///   1. Compute fixed coupon = notional × fixedRate × tau
///   2. Compute floating coupon = notional × forwardRate × tau (from curve)
///   3. Net: payer pays (fixed - floating) if positive
///
/// Then net across all swaps between each bilateral pair.
pub fn generate_settlement_instructions(
    swaps: &[SwapTerms],
    payment_date: u64,
    rates: &[Ray],
    settlement: u64,
) -> Vec<SettlementInstruction> {
    let dfs = match bootstrap_annual(rates) {
        Ok(dfs) => dfs,
        Err(_) => return vec![],
    };

    // Compute per-swap coupon payments
    let mut coupons: Vec<CouponPayment> = Vec::new();

    for swap in swaps {
        // Find the period that pays on this date
        for i in 0..swap.payment_dates.len() {
            if swap.payment_dates[i] != payment_date { continue; }

            let period_start = if i == 0 { swap.settlement } else { swap.payment_dates[i - 1] };
            let period_end = swap.payment_dates[i];
            let conv = swap.currency.convention();
            let tau = year_fraction(conv.day_count, period_start, period_end);
            let tau_f64 = ray_to_f64(tau);

            // Fixed coupon
            let fixed = ray_to_f64(swap.notional) * ray_to_f64(swap.fixed_rate) * tau_f64;

            // Floating coupon (forward rate from curve)
            let df_start = if i == 0 { RAY } else if i <= dfs.len() { dfs[i - 1] } else { *dfs.last().unwrap() };
            let df_end = if i <= dfs.len() { dfs[i.min(dfs.len() - 1)] } else { *dfs.last().unwrap() };
            let fwd = if tau > 0 && df_end > 0 {
                ray_to_f64(forward_rate(df_start, df_end, tau))
            } else {
                0.0
            };
            let floating = ray_to_f64(swap.notional) * fwd * tau_f64;

            // Net: payer pays fixed, receives floating
            let net = fixed - floating;

            coupons.push(CouponPayment {
                swap_ref: swap.id,
                payer: swap.payer,
                receiver: swap.receiver,
                fixed_amount: fixed,
                floating_amount: floating,
                net,
            });
        }
    }

    // Net across bilateral pairs
    let mut pair_map: std::collections::HashMap<(usize, usize), Vec<&CouponPayment>> =
        std::collections::HashMap::new();

    for c in &coupons {
        let key = if c.payer < c.receiver {
            (c.payer, c.receiver)
        } else {
            (c.receiver, c.payer)
        };
        pair_map.entry(key).or_default().push(c);
    }

    let mut instructions = Vec::new();

    for (&(a, b), payments) in &pair_map {
        let mut net_a_pays_b = 0.0f64; // positive = A owes B
        let mut total_fixed = 0.0f64;
        let mut total_floating = 0.0f64;
        let mut refs = Vec::new();

        for p in payments {
            if p.payer == a {
                net_a_pays_b += p.net;
            } else {
                net_a_pays_b -= p.net;
            }
            total_fixed += p.fixed_amount;
            total_floating += p.floating_amount;
            refs.push(p.swap_ref);
        }

        let (payer, receiver, amount) = if net_a_pays_b >= 0.0 {
            (a, b, net_a_pays_b)
        } else {
            (b, a, -net_a_pays_b)
        };

        if amount > 0.01 { // skip dust
            instructions.push(SettlementInstruction {
                payment_date,
                payer,
                receiver,
                currency: swaps[0].currency,
                net_amount: amount,
                fixed_gross: total_fixed,
                floating_gross: total_floating,
                swap_refs: refs,
            });
        }
    }

    instructions
}

/// Swap terms needed for settlement computation.
#[derive(Debug, Clone)]
pub struct SwapTerms {
    pub id: usize,
    pub payer: usize,
    pub receiver: usize,
    pub notional: Ray,
    pub fixed_rate: Ray,
    pub currency: Currency,
    pub settlement: u64,
    pub payment_dates: Vec<u64>,
}

/// Generate all upcoming payment dates across a set of swaps.
pub fn upcoming_payments(swaps: &[SwapTerms], from: u64, days: u64) -> Vec<u64> {
    let until = from + days * 86400;
    let mut dates: Vec<u64> = swaps.iter()
        .flat_map(|s| s.payment_dates.iter())
        .filter(|&&d| d > from && d <= until)
        .copied()
        .collect();
    dates.sort();
    dates.dedup();
    dates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::bps_to_ray;
    use crate::schedule::ymd_to_unix;

    #[test]
    fn test_settlement_instruction() {
        let settle = ymd_to_unix(2026, 3, 20);
        let day: u64 = 86400;
        let dates: Vec<u64> = (1..=5).map(|i| settle + i * 365 * day).collect();

        let swap = SwapTerms {
            id: 0,
            payer: 0,
            receiver: 1,
            notional: 10_000_000 * RAY,
            fixed_rate: bps_to_ray(300), // 3%
            currency: Currency::GBP,
            settlement: settle,
            payment_dates: dates.clone(),
        };

        let rates = vec![bps_to_ray(300); 5];
        let instructions = generate_settlement_instructions(
            &[swap], dates[0], &rates, settle,
        );

        // At par rate, fixed = floating → net ≈ 0
        // Small residual from day count
        assert!(instructions.is_empty() || instructions[0].net_amount < 1000.0,
            "At par, net should be ~0, got {:?}", instructions.first().map(|i| i.net_amount));
    }

    #[test]
    fn test_settlement_off_market() {
        let settle = ymd_to_unix(2026, 3, 20);
        let day: u64 = 86400;
        let dates: Vec<u64> = (1..=5).map(|i| settle + i * 365 * day).collect();

        let swap = SwapTerms {
            id: 0,
            payer: 0,
            receiver: 1,
            notional: 10_000_000 * RAY,
            fixed_rate: bps_to_ray(400), // pays 4%
            currency: Currency::GBP,
            settlement: settle,
            payment_dates: dates.clone(),
        };

        // Market at 3% → payer pays 4%, receives 3% → payer owes 1% × 10M = 100K
        let rates = vec![bps_to_ray(300); 5];
        let instructions = generate_settlement_instructions(
            &[swap], dates[0], &rates, settle,
        );

        assert_eq!(instructions.len(), 1);
        assert_eq!(instructions[0].payer, 0);
        assert_eq!(instructions[0].receiver, 1);
        // Net ≈ 100,000 (1% of 10M)
        assert!((instructions[0].net_amount - 100_000.0).abs() < 5000.0,
            "Net should be ~100K, got {:.0}", instructions[0].net_amount);
    }

    #[test]
    fn test_bilateral_netting() {
        let settle = ymd_to_unix(2026, 3, 20);
        let day: u64 = 86400;
        let dates: Vec<u64> = (1..=3).map(|i| settle + i * 365 * day).collect();

        // Swap 1: Bank 0 pays 4% to Bank 1
        // Swap 2: Bank 1 pays 3% to Bank 0
        // Net: Bank 0 pays (4% - 3%) × notional
        let swaps = vec![
            SwapTerms {
                id: 0, payer: 0, receiver: 1,
                notional: 10_000_000 * RAY, fixed_rate: bps_to_ray(400),
                currency: Currency::GBP, settlement: settle,
                payment_dates: dates.clone(),
            },
            SwapTerms {
                id: 1, payer: 1, receiver: 0,
                notional: 10_000_000 * RAY, fixed_rate: bps_to_ray(300),
                currency: Currency::GBP, settlement: settle,
                payment_dates: dates.clone(),
            },
        ];

        let rates = vec![bps_to_ray(350); 3]; // market at 3.5%
        let instructions = generate_settlement_instructions(
            &swaps, dates[0], &rates, settle,
        );

        // Should be exactly 1 netted instruction
        assert_eq!(instructions.len(), 1, "Should net to 1 instruction");
    }

    #[test]
    fn test_upcoming_payments() {
        let settle = ymd_to_unix(2026, 3, 20);
        let day: u64 = 86400;
        let swap = SwapTerms {
            id: 0, payer: 0, receiver: 1,
            notional: RAY, fixed_rate: bps_to_ray(300),
            currency: Currency::GBP, settlement: settle,
            payment_dates: vec![settle + 180 * day, settle + 365 * day, settle + 730 * day],
        };

        let upcoming = upcoming_payments(&[swap], settle, 200);
        assert_eq!(upcoming.len(), 1); // only the 180-day payment is within 200 days
    }
}
