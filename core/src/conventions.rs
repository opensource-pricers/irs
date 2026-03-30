/// Currency and convention definitions per SPEC-001 §5.

/// Supported currencies (System Currency ID).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Currency {
    USD = 0, // SOFR
    EUR = 1, // ESTR
    GBP = 2, // SONIA
    JPY = 3, // TONA
    CHF = 4, // SARON
    AUD = 5, // AONIA
    CAD = 6, // CORRA
    SEK = 7, // SWESTR
}

/// Day count conventions per SPEC-001 §5.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DayCountConvention {
    Act360 = 0,
    Act365Fixed = 1,
    Thirty360BondBasis = 2,
    ThirtyE360 = 3,
    // Phase 2:
    // ActActISDA = 4,
    // ActActICMA = 5,
}

/// Per-currency convention parameters.
#[derive(Debug, Clone, Copy)]
pub struct CurrencyConvention {
    pub day_count: DayCountConvention,
    pub payments_per_year: u8,
}

impl Currency {
    pub fn convention(self) -> CurrencyConvention {
        match self {
            Currency::USD => CurrencyConvention { day_count: DayCountConvention::Act360, payments_per_year: 1 },
            Currency::EUR => CurrencyConvention { day_count: DayCountConvention::Act360, payments_per_year: 1 },
            Currency::GBP => CurrencyConvention { day_count: DayCountConvention::Act365Fixed, payments_per_year: 1 },
            Currency::JPY => CurrencyConvention { day_count: DayCountConvention::Act365Fixed, payments_per_year: 1 },
            Currency::CHF => CurrencyConvention { day_count: DayCountConvention::Act360, payments_per_year: 1 },
            Currency::AUD => CurrencyConvention { day_count: DayCountConvention::Act365Fixed, payments_per_year: 2 },
            Currency::CAD => CurrencyConvention { day_count: DayCountConvention::Act365Fixed, payments_per_year: 2 },
            Currency::SEK => CurrencyConvention { day_count: DayCountConvention::Act360, payments_per_year: 1 },
        }
    }

    pub fn is_semi_annual(self) -> bool {
        matches!(self, Currency::AUD | Currency::CAD)
    }

    /// Returns the canonical OIS index name for this currency.
    pub fn index_name(self) -> &'static str {
        match self {
            Currency::USD => "SOFR",
            Currency::EUR => "ESTR",
            Currency::GBP => "SONIA",
            Currency::JPY => "TONA",
            Currency::CHF => "SARON",
            Currency::AUD => "AONIA",
            Currency::CAD => "CORRA",
            Currency::SEK => "SWESTR",
        }
    }

    /// Validate that a given index name is the supported OIS index for this currency.
    /// Returns false for IBOR/tenor indices (EURIBOR, LIBOR, TIBOR, etc.).
    pub fn validate_index(self, index: &str) -> bool {
        index.eq_ignore_ascii_case(self.index_name())
    }

    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Currency::USD),
            1 => Some(Currency::EUR),
            2 => Some(Currency::GBP),
            3 => Some(Currency::JPY),
            4 => Some(Currency::CHF),
            5 => Some(Currency::AUD),
            6 => Some(Currency::CAD),
            7 => Some(Currency::SEK),
            _ => None,
        }
    }
}
