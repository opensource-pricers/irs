# IRS — Open Source Interest Rate Swap Pricer

Rust implementation of OIS (Overnight Index Swap) discount factor bootstrap and swap valuation. Built for [CheckMySwap](https://www.checkmyswap.com) — a free tool for verifying bank swap marks against real interbank trade data.

## Why this exists

When a bank tells you your swap is worth -$4.2M, how do you check? You need an OIS discount curve, a bootstrap algorithm, and a pricer. This library provides all three, with the same precision as institutional pricing systems.

The production pricer at [checkmyswap.com](https://www.checkmyswap.com) uses a JavaScript implementation of the same algorithm. Both have been verified to produce identical results to machine epsilon (1e-16) across all 5 currencies, spot and forward-starting swaps.

## Quick example

```rust
use swap_core::conventions::Currency;

let conv = Currency::USD.convention();
// conv.settlement_days = 2
// conv.basis = 360
// conv.day_count = ACT/360
// conv.business_day_convention = ModifiedFollowing

// 1. Generate calendar-adjusted payment dates (caller's responsibility)
// 2. Compute year fractions: actual_days / basis
// 3. Bootstrap discount factors from par OIS rates
// 4. Price any swap using the bootstrapped DFs
```

## What's inside

### `core/` — Pricing library

| Module | Description |
|---|---|
| `bootstrap.rs` | OIS discount factor bootstrap with Brent solver for non-consecutive tenors |
| `math.rs` | RAY fixed-point arithmetic (1e27 precision, U256 intermediates) |
| `daycount.rs` | ACT/360, ACT/365F, 30/360, 30E/360 |
| `conventions.rs` | Per-currency conventions: day count, settlement days, basis, frequency |
| `interpolation.rs` | Log-linear discount factor interpolation |
| `schedule.rs` | Payment date generation (annual, semi-annual, quarterly, monthly) |
| `cashflow.rs` | Fixed, floating, notional exchange, conditional cash flows |
| `leg.rs` | Swap leg builder with amortizing and step-rate support |
| `products.rs` | Templates: IRS, FRA, FRN, fixed-rate bond, xccy swap, zero-coupon swap |
| `valuation.rs` | PV, DV01, bucket DV01, gamma, theta, portfolio valuation |
| `stress.rs` | 8 predefined scenarios (parallel shift, steepener, flattener, etc.) |
| `settlement.rs` | Settlement instructions with bilateral netting |
| `fixings.rs` | Overnight rate compounding for floating legs |

### `solana/` — On-chain program (deployment planned)

Solana BPF program for verifiable curve publication and swap valuation on-chain. The intent is to deploy this contract on Solana mainnet so that any curve publication and swap valuation can be independently verified on-chain — removing the need to trust a single pricing provider.

| Module | Description |
|---|---|
| `instruction.rs` | 12 instructions: BootstrapDirect, PublishCurve, ActivateCurve, CreateSwap, RevalueSwap, etc. |
| `processor.rs` | All instruction handlers with PDA verification |
| `state.rs` | On-chain accounts: CurveSnapshot, SwapAccount, ReconciliationAccount |
| `pda.rs` | Program Derived Address derivation for curves, swaps, attestations |
| `error.rs` | Error types |

## Bootstrap algorithm

For **consecutive tenors** (1Y→2Y→3Y→4Y→5Y), the bootstrap is algebraic — no iteration needed:

```
DF[n] = (1 - S[n] × annuitySum) / (1 + S[n] × τ[n])
```

For **non-consecutive tenors** (5Y→7Y, 7Y→10Y, 10Y→15Y), a Brent solver finds DF[n] such that the par swap values zero. At each iteration, intermediate discount factors (e.g., 6Y between 5Y and 7Y) are re-interpolated log-linearly, ensuring consistency between the interpolated DFs and the final bootstrapped DF.

This approach is more accurate than pre-interpolating intermediates before bootstrapping, because it avoids fixing DFs at intermediate dates before the longer-tenor DF is known.

Interpolation between bootstrapped nodes is **log-linear on discount factors** — the same method used by major clearing houses.

## Precision

| Test | Result |
|---|---|
| Par rate round-trip (spot, all 5 currencies) | < 1e-12 bp |
| PV at forward par rate ($10M notional) | < $0.00005 |
| Spot/forward DF consistency | Exact to machine epsilon |
| Rust vs JavaScript (same calendar) | Identical to machine epsilon |

## Conventions

The library documents exact per-currency conventions but **does not embed holiday calendars**. The caller generates business-day-adjusted payment dates using their own calendar and passes them to the bootstrap and pricing functions.

| Currency | Index | Day count | Basis | Settlement | Frequency | Business day rule |
|---|---|---|---|---|---|---|
| USD | SOFR | ACT/360 | 360 | T+2 | Annual | Modified Following |
| EUR | ESTR | ACT/360 | 360 | T+2 | Annual | Modified Following |
| GBP | SONIA | ACT/365F | 365 | T+0 | Annual | Modified Following |
| JPY | TONA | ACT/365F | 365 | T+2 | Annual | Modified Following |
| CHF | SARON | ACT/360 | 360 | T+2 | Annual | Modified Following |

Conventions sourced from [OpenGamma Strata](https://strata.opengamma.io/apidocs/com/opengamma/strata/product/swap/type/FixedOvernightSwapConventions.html) and ISDA 2006 definitions.

### Calendar integration example

```text
let conv = Currency::USD.convention();
let settle = add_business_days(today, conv.settlement_days, &us_calendar);
for y in 1..=tenor {
    let adjusted = modified_following(add_years(settle, y), &us_calendar);
    payment_dates.push(adjusted);
}
// Pass adjusted dates to bootstrap/pricing
```

## Build and test

```bash
cargo build -p swap-core
cargo test -p swap-core
```

The `match_js` test verifies the Rust bootstrap against the production JavaScript pricer for USD, GBP, and CHF with calendar-adjusted payment dates.

## Data sources

The curves served at [checkmyswap.com](https://www.checkmyswap.com) are derived from:

| Currency | Source | Type |
|---|---|---|
| USD, EUR | [DTCC CFTC Public Swap Data](https://pddata.dtcc.com/) | Actual interbank swap trades (Dodd-Frank mandated, free) |
| GBP, JPY, CHF | [Eurex Clearing Settlement Prices](https://www.eurex.com/ec-en/clear/eurex-otc-clear/settlement-prices) | Official clearing house settlement (free) |

Updated every business day. Historical curves archived permanently.

## Limitations

- No embedded holiday calendar — by design, the caller provides adjusted dates
- No convexity adjustment on cross-currency basis swaps
- Solana program currently on devnet — mainnet deployment planned
- No holiday-adjusted schedule generation (use a calendar library like `bdays` or `chrono` with holiday data)

## License

MIT
