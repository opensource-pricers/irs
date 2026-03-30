use solana_program::program_error::ProgramError;

#[derive(Debug)]
pub enum SwapError {
    InvalidInstruction,
    InvalidCurrency,
    EmptyRates,
    LengthMismatch,
    BootstrapFailed,
    CurveNotActive,
    CurveExpired,
    Unauthorized,
    AlreadySettled,
}

impl From<SwapError> for ProgramError {
    fn from(e: SwapError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
