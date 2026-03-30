# IRS — Open Source Interest Rate Swap Pricer

Rust implementation of OIS (Overnight Index Swap) discount factor bootstrap and swap valuation.

The production pricer at [CheckMySwap](https://www.checkmyswap.com) uses a JavaScript implementation of the same algorithm. Both have been verified to produce identical results to machine epsilon (1e-16) across all currencies, spot and forward-starting swaps.

## What's inside

### `core/` — Pricing library

| Module | What it does |
|---|---|
| `bootstrap.rs` | OIS discount factor bootstrap: `DF[n] = (1 - S[n] × annuity) / (1 + S[n] × τ)` |
| `math.rs` | RAY fixed-point arithmetic (1e27 precision, U256 intermediates) |
| `daycount.rs` | ACT/360, ACT/365F, 30/360, 30E/360 |
| `conventions.rs` | Per-currency conventions: day count, settlement days, basis, payment frequency |
| `interpolation.rs` | Log-linear discount factor interpolation |
| `schedule.rs` | Payment date generation (annual, semi-annual, quarterly, monthly) |
| `cashflow.rs` | Fixed, floating, notional exchange, conditional cash flows |
| `leg.rs` | Swap leg builder with amortizing and step-rate support |
| `products.rs` | Templates: IRS, FRA, FRN, fixed-rate bond, xccy swap, zero-coupon swap |
| `valuation.rs` | PV, DV01, bucket DV01, gamma, theta, portfolio valuation |
| `stress.rs` | 8 predefined scenarios (parallel shift, steepener, flattener, etc.) |
| `settlement.rs` | Settlement instructions with bilateral netting |
| `fixings.rs` | Overnight rate compounding for floating legs |

### `solana/` — On-chain program

Solana BPF program for verifiable curve publication and swap valuation.

| Module | What it does |
|---|---|
| `instruction.rs` | 12 instructions: BootstrapDirect, PublishCurve, ActivateCurve, CreateSwap, RevalueSwap, etc. |
| `processor.rs` | All instruction handlers with PDA verification |
| `state.rs` | On-chain accounts: CurveSnapshot, SwapAccount, ReconciliationAccount |
| `pda.rs` | Program Derived Address derivation for curves, swaps, attestations |
| `error.rs` | Error types |

## Bootstrap algorithm

The bootstrap solves the par swap condition algebraically at each tenor:

```
S[n] × Σ(τ[i] × DF[i], i=1..n) = 1 - DF[n]

⟹ DF[n] = (1 - S[n] × annuitySum) / (1 + S[n] × τ[n])
```

No iteration. No solver. Algebraically exact.

For intermediate tenors (e.g., 6Y between the 5Y and 7Y nodes), discount factors are log-linearly interpolated.

## Precision

| Test | Result |
|---|---|
| Par rate round-trip (spot, all currencies) | < 1e-15 (machine epsilon) |
| Par rate round-trip (forward start) | PV = $0.000000 at par rate |
| Rust vs JavaScript (with same calendar) | Identical to machine epsilon |

## Conventions

The library provides exact per-currency conventions but **does not embed holiday calendars**. The caller is responsible for generating business-day-adjusted payment dates.

| Currency | Index | Day count | Basis | Settlement | Frequency | Business day rule |
|---|---|---|---|---|---|---|
| USD | SOFR | ACT/360 | 360 | T+2 | Annual | Modified Following |
| EUR | ESTR | ACT/360 | 360 | T+2 | Annual | Modified Following |
| GBP | SONIA | ACT/365F | 365 | T+0 | Annual | Modified Following |
| JPY | TONA | ACT/365F | 365 | T+2 | Annual | Modified Following |
| CHF | SARON | ACT/360 | 360 | T+2 | Annual | Modified Following |

Conventions sourced from [OpenGamma Strata](https://strata.opengamma.io/apidocs/com/opengamma/strata/product/swap/type/FixedOvernightSwapConventions.html) and ISDA 2006 definitions.

### Calendar integration

```text
let conv = Currency::USD.convention();
let settle = add_business_days(today, conv.settlement_days, &us_calendar);
for y in 1..=tenor {
    let adjusted = modified_following(add_years(settle, y), &us_calendar);
    payment_dates.push(adjusted);
}
// Pass adjusted dates to bootstrap/pricing — the core math is calendar-agnostic
```

## Build

```bash
cargo build -p swap-core
cargo test -p swap-core
```

## Data sources

The curves served at [checkmyswap.com](https://www.checkmyswap.com) come from:

| Currency | Source | Type |
|---|---|---|
| USD, EUR | [DTCC CFTC Public Swap Data](https://pddata.dtcc.com/) | Actual interbank trades (Dodd-Frank mandated) |
| GBP, JPY, CHF | [Eurex Clearing Settlement Prices](https://www.eurex.com/ec-en/clear/eurex-otc-clear/settlement-prices) | Official clearing house settlement |

All sources are free, public, and updated daily.

## Limitations

- No embedded holiday calendar (by design — caller provides adjusted dates)
- No convexity adjustment on cross-currency basis swaps
- Solana program deployed on devnet only

## License

MIT
