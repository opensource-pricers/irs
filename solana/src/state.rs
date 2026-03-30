/// On-chain account state per SPEC-002 (CurveStore), SPEC-006 (Swaps, Reconciliation).
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::pubkey::Pubkey;

/// Maximum tenor points per curve.
pub const MAX_TENORS: usize = 30;
/// Maximum payment dates per swap.
pub const MAX_PAYMENTS: usize = 60;
/// Challenge period in seconds (15 minutes).
pub const CHALLENGE_PERIOD: i64 = 900;

/// Curve publication status.
#[derive(BorshSerialize, BorshDeserialize, Debug, Clone, Copy, PartialEq)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum CurveStatus {
    Pending = 0,
    Active = 1,
    Vetoed = 2,
}

/// Stored curve snapshot per SPEC-002 §5.1.
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct CurveSnapshot {
    /// Is this account initialized?
    pub is_initialized: bool,
    /// System currency ID (0-7).
    pub currency: u8,
    /// Business date as YYYYMMDD integer.
    pub business_date: u32,
    /// Unix timestamp of publication.
    pub publish_timestamp: i64,
    /// Status: PENDING, ACTIVE, VETOED.
    pub status: CurveStatus,
    /// True if correction of a previous snapshot.
    pub correction_flag: bool,
    /// True if any tenor used a fallback source.
    pub fallback_flag: bool,
    /// Number of tenor points.
    pub num_tenors: u8,
    /// Tenor in calendar days.
    pub tenor_days: [u32; MAX_TENORS],
    /// Par swap rates in RAY (u128).
    pub rates: [u128; MAX_TENORS],
}

impl CurveSnapshot {
    pub const LEN: usize = 1 + 1 + 4 + 8 + 1 + 1 + 1 + 1 + (4 * MAX_TENORS) + (16 * MAX_TENORS);
}

/// Latest active curve pointer per currency.
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct CurveLatest {
    pub is_initialized: bool,
    pub currency: u8,
    pub latest_business_date: u32,
    pub curve_account: Pubkey,
}

impl CurveLatest {
    pub const LEN: usize = 1 + 1 + 4 + 32;
}

/// Bilateral swap per SPEC-006.
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct SwapAccount {
    pub is_initialized: bool,
    pub payer: Pubkey,
    pub receiver: Pubkey,
    pub notional: u128,
    pub fixed_rate: u128,
    pub currency: u8,
    pub settlement: u64,
    pub num_payments: u8,
    pub payment_dates: [u64; MAX_PAYMENTS],
    pub active: bool,
    pub last_mtm: i128,
    pub last_reval_time: i64,
}

impl SwapAccount {
    pub const LEN: usize = 1 + 32 + 32 + 16 + 16 + 1 + 8 + 1 + (8 * MAX_PAYMENTS) + 1 + 16 + 8;
}

/// Reconciliation commitment per SPEC-006 §2.
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct ReconciliationEntry {
    pub is_initialized: bool,
    pub bank_a: Pubkey,
    pub bank_b: Pubkey,
    pub curve_date: u32,
    pub num_trades: u16,
    /// Commitments from bank A (keccak256 hashes).
    pub commitments_a: Vec<[u8; 32]>,
    /// Commitments from bank B.
    pub commitments_b: Vec<[u8; 32]>,
    /// Trade references (agreed bilaterally).
    pub trade_refs: Vec<[u8; 32]>,
    /// Match status per trade: 0=PENDING, 1=MATCHED, 2=MISMATCHED.
    pub match_status: Vec<u8>,
}

/// Maximum trades per reconciliation batch.
pub const MAX_RECON_TRADES: usize = 500;

/// Reconciliation with fixed-size arrays (no Vec — deterministic account size).
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct ReconciliationAccount {
    pub is_initialized: bool,
    pub bank_a: Pubkey,
    pub bank_b: Pubkey,
    pub curve_date: u32,
    pub num_trades: u16,
    pub a_submitted: bool,
    pub b_submitted: bool,
    /// Commitments from bank A.
    pub commitments_a: [[u8; 32]; MAX_RECON_TRADES],
    /// Commitments from bank B.
    pub commitments_b: [[u8; 32]; MAX_RECON_TRADES],
    /// Match status: 0=PENDING, 1=MATCHED, 2=MISMATCHED.
    pub match_status: [u8; MAX_RECON_TRADES],
    /// Number matched, mismatched.
    pub num_matched: u16,
    pub num_mismatched: u16,
}

impl ReconciliationAccount {
    pub const LEN: usize = 1 + 32 + 32 + 4 + 2 + 1 + 1
        + (32 * MAX_RECON_TRADES) + (32 * MAX_RECON_TRADES) + MAX_RECON_TRADES
        + 2 + 2;
}

/// Custodian attestation per SPEC-006 §4.
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct AttestationAccount {
    pub is_initialized: bool,
    pub custodian: Pubkey,
    pub client: Pubkey,
    pub curve_date: u32,
    pub timestamp: i64,
    pub num_trades: u32,
    pub net_mtm: i128,
    pub collateral_held: u128,
    pub collateralization_pct: u32, // basis points (12000 = 120%)
    pub num_disputes: u16,
}

impl AttestationAccount {
    pub const LEN: usize = 1 + 32 + 32 + 4 + 8 + 4 + 16 + 16 + 4 + 2;
}

/// Compression proposal per SPEC-006 §3.
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct CompressionProposal {
    pub is_initialized: bool,
    pub bank_a: Pubkey,
    pub bank_b: Pubkey,
    pub nonce: u64,
    pub timestamp: i64,
    pub risk_proof_hash: [u8; 32],
    pub notional_reduction: u128,
    pub trade_count_reduction: u16,
    pub a_approved: bool,
    pub b_approved: bool,
    pub executed: bool,
}

impl CompressionProposal {
    pub const LEN: usize = 1 + 32 + 32 + 8 + 8 + 32 + 16 + 2 + 1 + 1 + 1;
}

/// Oracle and guardian configuration.
#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct ProgramConfig {
    pub is_initialized: bool,
    pub oracle_authority: Pubkey,
    pub guardian_authority: Pubkey,
    pub admin: Pubkey,
}

impl ProgramConfig {
    pub const LEN: usize = 1 + 32 + 32 + 32;
}
