/// Program instructions per SPEC-002 (CurveStore), SPEC-003 (Bootstrap),
/// SPEC-005 (Valuation), SPEC-006 (Reconciliation).
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub enum SwapInstruction {
    /// Publish a curve snapshot (oracle only).
    /// Enters PENDING state for 15-minute challenge period.
    ///
    /// Accounts:
    ///   0. [signer]   Oracle authority
    ///   1. [writable]  CurveStore account (PDA)
    ///   2. []          System program
    PublishCurve {
        currency: u8,
        business_date: u32,     // YYYYMMDD
        tenor_days: Vec<u32>,   // tenor in calendar days
        rates: Vec<u128>,       // par swap rates in RAY
    },

    /// Activate a pending curve after challenge period.
    ///
    /// Accounts:
    ///   0. [writable]  CurveStore account (PDA)
    ActivateCurve {
        currency: u8,
        business_date: u32,
    },

    /// Veto a pending curve (guardian only).
    ///
    /// Accounts:
    ///   0. [signer]   Guardian authority
    ///   1. [writable]  CurveStore account (PDA)
    VetoCurve {
        currency: u8,
        business_date: u32,
    },

    /// Bootstrap discount factors from rates stored in CurveStore.
    /// Pure computation — result returned in return data.
    ///
    /// Accounts:
    ///   0. []  CurveStore account (PDA) — read only
    Bootstrap {
        currency: u8,
        business_date: u32,
        settlement: u64,          // Unix timestamp
        payment_dates: Vec<u64>,  // Unix timestamps
    },

    /// Bootstrap from provided rates (no CurveStore lookup).
    /// Pure computation — result returned in return data.
    ///
    /// Accounts: none required
    BootstrapDirect {
        currency: u8,
        settlement: u64,
        payment_dates: Vec<u64>,
        rates: Vec<u128>,
    },

    /// Create a bilateral IRS.
    ///
    /// Accounts:
    ///   0. [signer]    Payer (pays fixed)
    ///   1. []           Receiver
    ///   2. [writable]   Swap account (PDA)
    ///   3. []           CurveStore account (PDA)
    ///   4. []           System program
    CreateSwap {
        notional: u128,           // in RAY
        fixed_rate: u128,         // in RAY
        currency: u8,
        settlement: u64,
        payment_dates: Vec<u64>,
    },

    /// Revalue a swap with current market rates.
    ///
    /// Accounts:
    ///   0. [signer]    Payer or Receiver
    ///   1. [writable]   Swap account (PDA)
    ///   2. []           CurveStore account (PDA)
    RevalueSwap {
        business_date: u32,
    },

    /// Submit reconciliation commitment.
    ///
    /// Accounts:
    ///   0. [signer]    Submitting bank
    ///   1. [writable]   Reconciliation account (PDA)
    ///   2. []           System program
    SubmitCommitment {
        counterparty: [u8; 32],  // Pubkey bytes
        curve_date: u32,
        trade_refs: Vec<[u8; 32]>,
        commitments: Vec<[u8; 32]>,
    },

    /// Check reconciliation match status.
    ///
    /// Accounts:
    ///   0. []  Reconciliation account (PDA)
    CheckMatches {
        bank_a: [u8; 32],
        bank_b: [u8; 32],
        curve_date: u32,
    },

    /// Initialize program configuration (admin only, one-time).
    ///
    /// Accounts:
    ///   0. [signer]   Admin
    ///   1. [writable]  Config account (PDA)
    ///   2. []          System program
    InitializeConfig {
        oracle_authority: [u8; 32],
        guardian_authority: [u8; 32],
    },

    /// Publish custodian attestation per SPEC-006 §4.
    ///
    /// Accounts:
    ///   0. [signer]   Custodian authority
    ///   1. [writable]  Attestation account (PDA)
    ///   2. []          System program
    PublishAttestation {
        client: [u8; 32],
        curve_date: u32,
        num_trades: u32,
        net_mtm: i128,
        collateral_held: u128,
        collateralization_pct: u32,
        num_disputes: u16,
    },

    /// Propose portfolio compression per SPEC-006 §3.
    ///
    /// Accounts:
    ///   0. [signer]   Proposing bank
    ///   1. [writable]  Compression proposal account (PDA)
    ///   2. []          System program
    ProposeCompression {
        counterparty: [u8; 32],
        nonce: u64,
        risk_proof_hash: [u8; 32],
        notional_reduction: u128,
        trade_count_reduction: u16,
    },

    /// Approve a compression proposal. Both parties must approve.
    ///
    /// Accounts:
    ///   0. [signer]   Approving bank
    ///   1. [writable]  Compression proposal account (PDA)
    ApproveCompression,
}
