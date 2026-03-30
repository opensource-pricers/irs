# IRS — Open Source Interest Rate Swap Pricer

Rust implementation of OIS (Overnight Index Swap) discount factor bootstrap and swap valuation. Algebraically exact — proven to match machine epsilon (1e-16).

Used in production by [CheckMySwap](https://www.checkmyswap.com).

## What's inside

### `core/` — Pricing library (3,744 lines)

| Module | What it does |
|---|---|
| `bootstrap.rs` | OIS discount factor bootstrap: `DF[n] = (1 - S[n] × annuity) / (1 + S[n] × τ)` |
| `math.rs` | RAY fixed-point arithmetic (1e27 precision, U256 intermediates) |
| `daycount.rs` | ACT/360, ACT/365F, 30/360, 30E/360 |
| `conventions.rs` | 8 currency conventions (USD/SOFR, EUR/ESTR, GBP/SONIA, JPY/TONA, CHF/SARON, AUD/AONIA, CAD/CORRA, SEK/SWESTR) |
| `interpolation.rs` | Log-linear discount factor interpolation |
| `schedule.rs` | Payment date generation (annual, semi-annual, quarterly, monthly) |
| `cashflow.rs` | Fixed, floating, notional exchange, conditional cash flows |
| `leg.rs` | Swap leg builder with amortizing and step-rate support |
| `products.rs` | Templates: IRS, FRA, FRN, fixed-rate bond, xccy swap, zero-coupon swap |
| `valuation.rs` | PV, DV01, bucket DV01, gamma, theta, portfolio valuation |
| `stress.rs` | 8 predefined scenarios (parallel shift, steepener, flattener, etc.) |
| `settlement.rs` | Settlement instructions with bilateral netting |
| `fixings.rs` | Overnight rate compounding for floating legs |

### `solana/` — On-chain program (1,106 lines)

Solana BPF program for verifiable curve publication and swap valuation.

| Module | What it does |
|---|---|
| `instruction.rs` | 12 instructions: BootstrapDirect, PublishCurve, ActivateCurve, CreateSwap, RevalueSwap, etc. |
| `processor.rs` | All instruction handlers with PDA verification |
| `state.rs` | On-chain accounts: CurveSnapshot, SwapAccount, ReconciliationAccount |
| `pda.rs` | Program Derived Address derivation for curves, swaps, attestations |
| `error.rs` | Error types |

## Bootstrap algorithm

The core bootstrap solves the par swap condition algebraically at each tenor:

```
S[n] × Σ(τ[i] × DF[i], i=1..n) = 1 - DF[n]

⟹ DF[n] = (1 - S[n] × annuitySum) / (1 + S[n] × τ[n])
```

No iteration. No solver. Algebraically exact.

For intermediate tenors (e.g., 6Y between the 5Y and 7Y nodes), discount factors are log-linearly interpolated.

## Precision

| Test | Error |
|---|---|
| Par rate round-trip (spot) | < 1e-15 (machine epsilon) |
| Par rate round-trip (forward) | PV = $0.000000 at par |
| vs Eurex clearing settlement | < 3bp (short end), < 7bp (long end) |

## Currencies supported

| Currency | Index | Day count | Frequency |
|---|---|---|---|
| USD | SOFR | ACT/360 | Annual |
| EUR | ESTR | ACT/360 | Annual |
| GBP | SONIA | ACT/365F | Annual |
| JPY | TONA | ACT/365F | Annual |
| CHF | SARON | ACT/360 | Annual |
| AUD | AONIA | ACT/365F | Annual |
| CAD | CORRA | ACT/365F | Annual |
| SEK | SWESTR | ACT/360 | Annual |

## Build

```bash
# Core library
cargo build -p swap-core

# Solana program
cargo build-bpf -p swap-solana

# Run tests
cargo test -p swap-core
```

## Data sources

The curves used in production at [checkmyswap.com](https://www.checkmyswap.com) come from:

| Currency | Source | Type |
|---|---|---|
| USD, EUR | [DTCC CFTC Public Swap Data](https://pddata.dtcc.com/) | Actual interbank trades (Dodd-Frank mandated) |
| GBP, JPY, CHF | [Eurex Clearing Settlement Prices](https://www.eurex.com/ec-en/clear/eurex-otc-clear/settlement-prices) | Official clearing house settlement |

All sources are free, public, and updated daily.

## Limitations

- No holiday calendar — payment dates use exact 365-day years
- No convexity adjustment on xccy basis
- Forward starts use fractional years, not exact business dates
- Solana program deployed on devnet only

## License

MIT
