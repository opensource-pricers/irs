/// PDA derivation for all program accounts per SPEC-009 §4.
use solana_program::pubkey::Pubkey;

pub const CURVE_SEED: &[u8] = b"curve";
pub const CURVE_LATEST_SEED: &[u8] = b"curve_latest";
pub const SWAP_SEED: &[u8] = b"swap";
pub const RECON_SEED: &[u8] = b"recon";
pub const CONFIG_SEED: &[u8] = b"config";
pub const ATTESTATION_SEED: &[u8] = b"attest";
pub const COMPRESSION_SEED: &[u8] = b"compress";

/// CurveStore PDA: seeds = ["curve", currency, business_date]
pub fn curve_pda(program_id: &Pubkey, currency: u8, business_date: u32) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[CURVE_SEED, &[currency], &business_date.to_le_bytes()],
        program_id,
    )
}

/// Latest curve pointer PDA: seeds = ["curve_latest", currency]
pub fn curve_latest_pda(program_id: &Pubkey, currency: u8) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[CURVE_LATEST_SEED, &[currency]],
        program_id,
    )
}

/// Swap PDA: seeds = ["swap", payer, receiver, settlement_bytes]
/// Keys are sorted to ensure same PDA regardless of who creates.
pub fn swap_pda(
    program_id: &Pubkey,
    payer: &Pubkey,
    receiver: &Pubkey,
    settlement: u64,
) -> (Pubkey, u8) {
    let (first, second) = if payer < receiver {
        (payer, receiver)
    } else {
        (receiver, payer)
    };
    Pubkey::find_program_address(
        &[SWAP_SEED, first.as_ref(), second.as_ref(), &settlement.to_le_bytes()],
        program_id,
    )
}

/// Reconciliation PDA: seeds = ["recon", bank_a_sorted, bank_b_sorted, curve_date]
/// Keys sorted so both parties derive the same PDA.
pub fn recon_pda(
    program_id: &Pubkey,
    bank_a: &Pubkey,
    bank_b: &Pubkey,
    curve_date: u32,
) -> (Pubkey, u8) {
    let (first, second) = if bank_a < bank_b {
        (bank_a, bank_b)
    } else {
        (bank_b, bank_a)
    };
    Pubkey::find_program_address(
        &[RECON_SEED, first.as_ref(), second.as_ref(), &curve_date.to_le_bytes()],
        program_id,
    )
}

/// Program config PDA: seeds = ["config"]
pub fn config_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[CONFIG_SEED], program_id)
}

/// Attestation PDA: seeds = ["attest", custodian, client, curve_date]
pub fn attestation_pda(
    program_id: &Pubkey,
    custodian: &Pubkey,
    client: &Pubkey,
    curve_date: u32,
) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[ATTESTATION_SEED, custodian.as_ref(), client.as_ref(), &curve_date.to_le_bytes()],
        program_id,
    )
}

/// Compression proposal PDA: seeds = ["compress", bank_a, bank_b, proposal_nonce]
pub fn compression_pda(
    program_id: &Pubkey,
    bank_a: &Pubkey,
    bank_b: &Pubkey,
    nonce: u64,
) -> (Pubkey, u8) {
    let (first, second) = if bank_a < bank_b {
        (bank_a, bank_b)
    } else {
        (bank_b, bank_a)
    };
    Pubkey::find_program_address(
        &[COMPRESSION_SEED, first.as_ref(), second.as_ref(), &nonce.to_le_bytes()],
        program_id,
    )
}
