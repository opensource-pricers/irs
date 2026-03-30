/// Instruction processor per SPEC-002, SPEC-003, SPEC-005, SPEC-006.
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvar::Sysvar,
};

use swap_core::{
    bootstrap::bootstrap_ois,
    conventions::Currency,
    math::{ray_mul, ray_to_f64, Ray, RAY},
    daycount::year_fraction,
};

use crate::error::SwapError;
use crate::instruction::SwapInstruction;
use crate::state::*;

pub fn process(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    let instruction = SwapInstruction::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;

    match instruction {
        SwapInstruction::BootstrapDirect { currency, settlement, payment_dates, rates } =>
            process_bootstrap_direct(currency, settlement, &payment_dates, &rates),

        SwapInstruction::PublishCurve { currency, business_date, tenor_days, rates } =>
            process_publish_curve(program_id, accounts, currency, business_date, &tenor_days, &rates),

        SwapInstruction::ActivateCurve { currency, business_date } =>
            process_activate_curve(program_id, accounts, currency, business_date),

        SwapInstruction::VetoCurve { currency, business_date } =>
            process_veto_curve(program_id, accounts, currency, business_date),

        SwapInstruction::CreateSwap { notional, fixed_rate, currency, settlement, payment_dates } =>
            process_create_swap(program_id, accounts, notional, fixed_rate, currency, settlement, &payment_dates),

        SwapInstruction::RevalueSwap { business_date } =>
            process_revalue_swap(program_id, accounts, business_date),

        SwapInstruction::SubmitCommitment { counterparty, curve_date, trade_refs, commitments } =>
            process_submit_commitment(program_id, accounts, &counterparty, curve_date, &trade_refs, &commitments),

        SwapInstruction::CheckMatches { bank_a, bank_b, curve_date } =>
            process_check_matches(program_id, accounts, &bank_a, &bank_b, curve_date),

        SwapInstruction::InitializeConfig { oracle_authority, guardian_authority } =>
            process_init_config(program_id, accounts, &oracle_authority, &guardian_authority),

        SwapInstruction::PublishAttestation { client, curve_date, num_trades, net_mtm, collateral_held, collateralization_pct, num_disputes } =>
            process_publish_attestation(program_id, accounts, &client, curve_date, num_trades, net_mtm, collateral_held, collateralization_pct, num_disputes),

        SwapInstruction::ProposeCompression { counterparty, nonce, risk_proof_hash, notional_reduction, trade_count_reduction } =>
            process_propose_compression(program_id, accounts, &counterparty, nonce, &risk_proof_hash, notional_reduction, trade_count_reduction),

        SwapInstruction::ApproveCompression =>
            process_approve_compression(program_id, accounts),

        _ => {
            msg!("Instruction not implemented");
            Err(ProgramError::InvalidInstructionData)
        }
    }
}

// ========================================================================
//  BOOTSTRAP
// ========================================================================

fn process_bootstrap_direct(
    currency: u8, settlement: u64, payment_dates: &[u64], rates: &[u128],
) -> ProgramResult {
    let ccy = Currency::from_u8(currency).ok_or(SwapError::InvalidCurrency)?;
    let dfs = bootstrap_ois(rates, payment_dates, settlement, ccy)
        .map_err(|_| SwapError::BootstrapFailed)?;

    msg!("Bootstrapped {} DFs for currency {}", dfs.len(), currency);
    let mut data = Vec::with_capacity(dfs.len() * 16);
    for df in &dfs {
        data.extend_from_slice(&df.to_le_bytes());
    }
    solana_program::program::set_return_data(&data);
    Ok(())
}

// ========================================================================
//  CURVE STORE
// ========================================================================

fn process_publish_curve(
    program_id: &Pubkey, accounts: &[AccountInfo],
    currency: u8, business_date: u32, tenor_days: &[u32], rates: &[u128],
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let oracle = next_account_info(accounts_iter)?;
    let curve_account = next_account_info(accounts_iter)?;

    if !oracle.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    verify_owner(curve_account, program_id)?;
    verify_pda(
        curve_account,
        &[b"curve", &[currency], &business_date.to_le_bytes()],
        program_id,
    )?;
    if tenor_days.len() != rates.len() || tenor_days.is_empty() || tenor_days.len() > MAX_TENORS {
        return Err(SwapError::LengthMismatch.into());
    }

    let clock = Clock::get()?;
    let mut snapshot = CurveSnapshot {
        is_initialized: true, currency, business_date,
        publish_timestamp: clock.unix_timestamp,
        status: CurveStatus::Pending,
        correction_flag: false, fallback_flag: false,
        num_tenors: tenor_days.len() as u8,
        tenor_days: [0u32; MAX_TENORS],
        rates: [0u128; MAX_TENORS],
    };
    for (i, (&td, &r)) in tenor_days.iter().zip(rates.iter()).enumerate() {
        snapshot.tenor_days[i] = td;
        snapshot.rates[i] = r;
    }

    write_account(&snapshot, curve_account)?;
    msg!("Curve published: ccy={}, date={}, status=PENDING", currency, business_date);
    Ok(())
}

fn process_activate_curve(
    program_id: &Pubkey, accounts: &[AccountInfo], currency: u8, business_date: u32,
) -> ProgramResult {
    let curve_account = &accounts[0];
    // No signer check: per SPEC-002 §5.2, callable by anyone after challenge period
    verify_owner(curve_account, program_id)?;
    let mut snapshot: CurveSnapshot = read_account(curve_account)?;

    if snapshot.status != CurveStatus::Pending {
        return Err(SwapError::CurveNotActive.into());
    }
    let clock = Clock::get()?;
    if clock.unix_timestamp < snapshot.publish_timestamp + CHALLENGE_PERIOD {
        msg!("Challenge period not elapsed");
        return Err(ProgramError::InvalidArgument);
    }

    snapshot.status = CurveStatus::Active;
    write_account(&snapshot, curve_account)?;
    msg!("Curve activated: ccy={}, date={}", currency, business_date);
    Ok(())
}

fn process_veto_curve(
    program_id: &Pubkey, accounts: &[AccountInfo], currency: u8, business_date: u32,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let guardian = next_account_info(accounts_iter)?;
    let curve_account = next_account_info(accounts_iter)?;

    if !guardian.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    verify_owner(curve_account, program_id)?;

    let mut snapshot: CurveSnapshot = read_account(curve_account)?;
    if snapshot.status != CurveStatus::Pending {
        return Err(SwapError::CurveNotActive.into());
    }
    let clock = Clock::get()?;
    if clock.unix_timestamp >= snapshot.publish_timestamp + CHALLENGE_PERIOD {
        msg!("Challenge period elapsed — cannot veto");
        return Err(ProgramError::InvalidArgument);
    }

    snapshot.status = CurveStatus::Vetoed;
    write_account(&snapshot, curve_account)?;
    msg!("Curve VETOED: ccy={}, date={}", currency, business_date);
    Ok(())
}

// ========================================================================
//  SWAP LIFECYCLE
// ========================================================================

fn process_create_swap(
    program_id: &Pubkey, accounts: &[AccountInfo], notional: u128, fixed_rate: u128,
    currency: u8, settlement: u64, payment_dates: &[u64],
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let payer = next_account_info(accounts_iter)?;
    let receiver = next_account_info(accounts_iter)?;
    let swap_account = next_account_info(accounts_iter)?;
    let curve_account = next_account_info(accounts_iter)?;

    if !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    verify_owner(swap_account, program_id)?;
    verify_owner(curve_account, program_id)?;
    verify_pda(
        swap_account,
        &[b"swap", payer.key.as_ref(), receiver.key.as_ref(), &settlement.to_le_bytes()],
        program_id,
    )?;
    if payment_dates.len() > MAX_PAYMENTS {
        return Err(SwapError::LengthMismatch.into());
    }

    // Read active curve and bootstrap DFs
    let curve: CurveSnapshot = read_account(curve_account)?;
    if curve.status != CurveStatus::Active {
        return Err(SwapError::CurveNotActive.into());
    }

    let ccy = Currency::from_u8(currency).ok_or(SwapError::InvalidCurrency)?;
    let rates: Vec<u128> = curve.rates[..curve.num_tenors as usize].to_vec();
    let dfs = bootstrap_ois(&rates, payment_dates, settlement, ccy)
        .map_err(|_| SwapError::BootstrapFailed)?;

    // Compute MTM: payer_MTM = Notional × (1 - DF_end - fixedRate × Annuity)
    let mtm = compute_swap_mtm(notional, fixed_rate, ccy, settlement, payment_dates, &dfs);

    let mut swap = SwapAccount {
        is_initialized: true,
        payer: *payer.key,
        receiver: *receiver.key,
        notional, fixed_rate, currency, settlement,
        num_payments: payment_dates.len() as u8,
        payment_dates: [0u64; MAX_PAYMENTS],
        active: true,
        last_mtm: mtm,
        last_reval_time: Clock::get()?.unix_timestamp,
    };
    for (i, &d) in payment_dates.iter().enumerate() {
        swap.payment_dates[i] = d;
    }

    write_account(&swap, swap_account)?;
    msg!("Swap created: notional={}, rate={:.4}%, MTM={}", notional, ray_to_f64(fixed_rate) * 100.0, mtm);
    Ok(())
}

fn process_revalue_swap(program_id: &Pubkey, accounts: &[AccountInfo], _business_date: u32) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let authority = next_account_info(accounts_iter)?;
    let swap_account = next_account_info(accounts_iter)?;
    let curve_account = next_account_info(accounts_iter)?;

    if !authority.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    verify_owner(swap_account, program_id)?;
    verify_owner(curve_account, program_id)?;

    let mut swap: SwapAccount = read_account(swap_account)?;
    if !swap.active {
        return Err(SwapError::AlreadySettled.into());
    }
    if *authority.key != swap.payer && *authority.key != swap.receiver {
        return Err(SwapError::Unauthorized.into());
    }

    let curve: CurveSnapshot = read_account(curve_account)?;
    if curve.status != CurveStatus::Active {
        return Err(SwapError::CurveNotActive.into());
    }

    let ccy = Currency::from_u8(swap.currency).ok_or(SwapError::InvalidCurrency)?;
    let n = swap.num_payments as usize;
    let payment_dates = &swap.payment_dates[..n];
    let rates: Vec<u128> = curve.rates[..curve.num_tenors as usize].to_vec();
    let dfs = bootstrap_ois(&rates, payment_dates, swap.settlement, ccy)
        .map_err(|_| SwapError::BootstrapFailed)?;

    let mtm = compute_swap_mtm(swap.notional, swap.fixed_rate, ccy, swap.settlement, payment_dates, &dfs);
    swap.last_mtm = mtm;
    swap.last_reval_time = Clock::get()?.unix_timestamp;

    write_account(&swap, swap_account)?;
    msg!("Swap revalued: MTM={}, VM={}", mtm, -mtm);
    Ok(())
}

/// Core MTM formula: Payer MTM = Notional × (1 - DF_end - fixedRate × Annuity)
fn compute_swap_mtm(
    notional: u128, fixed_rate: u128, ccy: Currency,
    settlement: u64, payment_dates: &[u64], dfs: &[Ray],
) -> i128 {
    let conv = ccy.convention();
    let n = dfs.len();
    let mut annuity: u128 = 0;
    let mut prev = settlement;
    for i in 0..n {
        let tau = year_fraction(conv.day_count, prev, payment_dates[i]);
        annuity += ray_mul(tau, dfs[i]);
        prev = payment_dates[i];
    }

    let df_end = dfs[n - 1];
    if df_end > RAY {
        msg!("Warning: DF > 1 (negative rates not supported)");
        return 0;
    }
    let float_pv = RAY as i128 - df_end as i128;
    let fixed_pv = ray_mul(fixed_rate, annuity) as i128;
    let mtm_per_unit = float_pv - fixed_pv;

    use ethnum::I256;
    let result = I256::from(mtm_per_unit) * I256::from(notional) / I256::from(RAY);
    result.as_i128()
}

// ========================================================================
//  RECONCILIATION
// ========================================================================

fn process_submit_commitment(
    program_id: &Pubkey, accounts: &[AccountInfo], counterparty: &[u8; 32], curve_date: u32,
    trade_refs: &[[u8; 32]], commitments: &[[u8; 32]],
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let bank = next_account_info(accounts_iter)?;
    let recon_account = next_account_info(accounts_iter)?;

    if !bank.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    verify_owner(recon_account, program_id)?;
    if trade_refs.len() != commitments.len() || trade_refs.is_empty() {
        return Err(SwapError::LengthMismatch.into());
    }
    if trade_refs.len() > MAX_RECON_TRADES {
        return Err(SwapError::LengthMismatch.into());
    }

    let cp = Pubkey::from(*counterparty);
    let (first, second) = if *bank.key < cp { (*bank.key, cp) } else { (cp, *bank.key) };
    let is_bank_a = *bank.key == first;

    verify_pda(
        recon_account,
        &[b"recon", first.as_ref(), second.as_ref(), &curve_date.to_le_bytes()],
        program_id,
    )?;

    let mut recon: ReconciliationAccount = if recon_account.data_len() > 0 {
        read_account(recon_account).unwrap_or(ReconciliationAccount {
            is_initialized: false,
            bank_a: first, bank_b: second, curve_date,
            num_trades: 0, a_submitted: false, b_submitted: false,
            commitments_a: [[0u8; 32]; MAX_RECON_TRADES],
            commitments_b: [[0u8; 32]; MAX_RECON_TRADES],
            match_status: [0u8; MAX_RECON_TRADES],
            num_matched: 0, num_mismatched: 0,
        })
    } else {
        ReconciliationAccount {
            is_initialized: false,
            bank_a: first, bank_b: second, curve_date,
            num_trades: 0, a_submitted: false, b_submitted: false,
            commitments_a: [[0u8; 32]; MAX_RECON_TRADES],
            commitments_b: [[0u8; 32]; MAX_RECON_TRADES],
            match_status: [0u8; MAX_RECON_TRADES],
            num_matched: 0, num_mismatched: 0,
        }
    };

    recon.is_initialized = true;
    recon.bank_a = first;
    recon.bank_b = second;
    recon.curve_date = curve_date;
    recon.num_trades = trade_refs.len() as u16;

    if is_bank_a {
        for (i, c) in commitments.iter().enumerate() {
            if i >= MAX_RECON_TRADES {
                return Err(SwapError::LengthMismatch.into());
            }
            recon.commitments_a[i] = *c;
        }
        recon.a_submitted = true;
    } else {
        for (i, c) in commitments.iter().enumerate() {
            if i >= MAX_RECON_TRADES {
                return Err(SwapError::LengthMismatch.into());
            }
            recon.commitments_b[i] = *c;
        }
        recon.b_submitted = true;
    }

    write_account(&recon, recon_account)?;
    msg!("Commitment submitted: {} trades, bank={}", trade_refs.len(), if is_bank_a { "A" } else { "B" });
    Ok(())
}

fn process_check_matches(
    program_id: &Pubkey, accounts: &[AccountInfo], _bank_a: &[u8; 32], _bank_b: &[u8; 32], _curve_date: u32,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let caller = next_account_info(accounts_iter)?;
    let recon_account = next_account_info(accounts_iter)?;

    if !caller.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    verify_owner(recon_account, program_id)?;

    let mut recon: ReconciliationAccount = read_account(recon_account)?;

    if !recon.a_submitted || !recon.b_submitted {
        msg!("Both parties must submit before matching");
        return Err(ProgramError::InvalidArgument);
    }

    let n = recon.num_trades as usize;
    let mut matched = 0u16;
    let mut mismatched = 0u16;

    for i in 0..n {
        if recon.commitments_a[i] == recon.commitments_b[i] {
            recon.match_status[i] = 1; // MATCHED
            matched += 1;
        } else {
            recon.match_status[i] = 2; // MISMATCHED
            mismatched += 1;
        }
    }

    recon.num_matched = matched;
    recon.num_mismatched = mismatched;

    write_account(&recon, recon_account)?;
    msg!("Reconciliation: {}/{} matched, {} mismatched", matched, n, mismatched);

    // Return summary via return data
    let mut data = Vec::with_capacity(6);
    data.extend_from_slice(&(n as u16).to_le_bytes());
    data.extend_from_slice(&matched.to_le_bytes());
    data.extend_from_slice(&mismatched.to_le_bytes());
    solana_program::program::set_return_data(&data);
    Ok(())
}

// ========================================================================
//  CONFIG
// ========================================================================

fn process_init_config(
    program_id: &Pubkey, accounts: &[AccountInfo], oracle: &[u8; 32], guardian: &[u8; 32],
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let admin = next_account_info(accounts_iter)?;
    let config_account = next_account_info(accounts_iter)?;

    if !admin.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    verify_owner(config_account, program_id)?;

    let config = ProgramConfig {
        is_initialized: true,
        oracle_authority: Pubkey::from(*oracle),
        guardian_authority: Pubkey::from(*guardian),
        admin: *admin.key,
    };

    write_account(&config, config_account)?;
    msg!("Config initialized: oracle={}, guardian={}", Pubkey::from(*oracle), Pubkey::from(*guardian));
    Ok(())
}

// ========================================================================
//  CUSTODIAN ATTESTATION
// ========================================================================

fn process_publish_attestation(
    program_id: &Pubkey, accounts: &[AccountInfo], client: &[u8; 32], curve_date: u32,
    num_trades: u32, net_mtm: i128, collateral_held: u128,
    collateralization_pct: u32, num_disputes: u16,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let custodian = next_account_info(accounts_iter)?;
    let attest_account = next_account_info(accounts_iter)?;

    if !custodian.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    verify_owner(attest_account, program_id)?;

    let attestation = AttestationAccount {
        is_initialized: true,
        custodian: *custodian.key,
        client: Pubkey::from(*client),
        curve_date,
        timestamp: Clock::get()?.unix_timestamp,
        num_trades,
        net_mtm,
        collateral_held,
        collateralization_pct,
        num_disputes,
    };

    write_account(&attestation, attest_account)?;
    msg!("Attestation: {} trades, MTM={}, collateral={}%, disputes={}",
        num_trades, net_mtm, collateralization_pct as f64 / 100.0, num_disputes);
    Ok(())
}

// ========================================================================
//  COMPRESSION
// ========================================================================

fn process_propose_compression(
    program_id: &Pubkey, accounts: &[AccountInfo], counterparty: &[u8; 32], nonce: u64,
    risk_proof_hash: &[u8; 32], notional_reduction: u128, trade_count_reduction: u16,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let proposer = next_account_info(accounts_iter)?;
    let proposal_account = next_account_info(accounts_iter)?;

    if !proposer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    verify_owner(proposal_account, program_id)?;

    let proposal = CompressionProposal {
        is_initialized: true,
        bank_a: *proposer.key,
        bank_b: Pubkey::from(*counterparty),
        nonce,
        timestamp: Clock::get()?.unix_timestamp,
        risk_proof_hash: *risk_proof_hash,
        notional_reduction,
        trade_count_reduction,
        a_approved: true,  // proposer auto-approves
        b_approved: false,
        executed: false,
    };

    write_account(&proposal, proposal_account)?;
    msg!("Compression proposed: reduction={}M notional, {} trades",
        notional_reduction / (1_000_000 * RAY), trade_count_reduction);
    Ok(())
}

fn process_approve_compression(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let approver = next_account_info(accounts_iter)?;
    let proposal_account = next_account_info(accounts_iter)?;

    if !approver.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    verify_owner(proposal_account, program_id)?;

    let mut proposal: CompressionProposal = read_account(proposal_account)?;
    if proposal.executed {
        return Err(SwapError::AlreadySettled.into());
    }

    if *approver.key == proposal.bank_a {
        proposal.a_approved = true;
    } else if *approver.key == proposal.bank_b {
        proposal.b_approved = true;
    } else {
        return Err(SwapError::Unauthorized.into());
    }

    if proposal.a_approved && proposal.b_approved {
        proposal.executed = true;
        msg!("Compression EXECUTED: {}M notional reduced", proposal.notional_reduction / (1_000_000 * RAY));
    } else {
        msg!("Compression approved by one party, awaiting counterparty");
    }

    write_account(&proposal, proposal_account)?;
    Ok(())
}

// ========================================================================
//  HELPERS
// ========================================================================

fn verify_pda(account: &AccountInfo, expected_seeds: &[&[u8]], program_id: &Pubkey) -> ProgramResult {
    let (expected_key, _) = Pubkey::find_program_address(expected_seeds, program_id);
    if account.key != &expected_key {
        msg!("PDA mismatch: expected {}, got {}", expected_key, account.key);
        return Err(ProgramError::InvalidSeeds);
    }
    Ok(())
}

fn verify_owner(account: &AccountInfo, program_id: &Pubkey) -> ProgramResult {
    if account.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }
    Ok(())
}

fn read_account<T: BorshDeserialize>(account: &AccountInfo) -> Result<T, ProgramError> {
    let data = account.try_borrow_data()?;
    T::try_from_slice(&data).map_err(|_| ProgramError::InvalidAccountData)
}

fn write_account<T: BorshSerialize>(state: &T, account: &AccountInfo) -> ProgramResult {
    let data = borsh::to_vec(state).map_err(|_| ProgramError::InvalidAccountData)?;
    let mut account_data = account.try_borrow_mut_data()?;
    if data.len() > account_data.len() {
        return Err(ProgramError::AccountDataTooSmall);
    }
    account_data[..data.len()].copy_from_slice(&data);
    Ok(())
}
