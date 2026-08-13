use thiserror::Error;

/// Crate-wide error type. Construction of any public numeric or
/// compositional type goes through validation that returns this error
/// instead of panicking (AGENTS.md §25: "panicを通常入力の処理に使用しない").
#[derive(Debug, Error, Clone, PartialEq)]
pub enum GugenError {
    #[error("range invalid: min ({min}) must be <= max ({max})")]
    InvalidRange { min: f64, max: f64 },
    #[error("{field} must be finite, got {value}")]
    NonFiniteValue { field: &'static str, value: f64 },
    #[error("{field} must be non-negative, got {value}")]
    NegativeMagnitude { field: &'static str, value: f64 },
    #[error("composition must contain at least one element")]
    EmptyComposition,
    #[error("'{0}' is not a recognized element symbol")]
    InvalidElementSymbol(String),
    #[error("amount for element {element} must be finite and > 0, got {amount}")]
    NonPositiveAmount { element: String, amount: f64 },
    #[error("element {element} was supplied more than once in the same composition")]
    DuplicateElement { element: String },
    #[error(
        "amount {value} for element {element} is not a simple rational number gugen can balance exactly (need a denominator <= 1_000_000 within tolerance 1e-9)"
    )]
    AmountNotRational { element: String, value: f64 },
    #[error("reaction species coefficients must be > 0")]
    ZeroCoefficient,
    #[error("Score01 must be finite and within [0, 1], got {value}")]
    ScoreOutOfRange { value: f64 },
    #[error("a reaction needs at least one reactant and one product")]
    EmptyReaction,
    #[error("exact arithmetic overflowed while balancing a reaction")]
    ArithmeticOverflow,
}

pub type Result<T> = std::result::Result<T, GugenError>;

pub(crate) fn require_finite(field: &'static str, value: f64) -> Result<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(GugenError::NonFiniteValue { field, value })
    }
}

/// Error returned by external data providers (AGENTS.md §8). Deliberately
/// separate from [`GugenError`]: one provider failing must not by itself
/// fail planning that doesn't depend on it (AGENTS.md §21.5).
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ProviderError {
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    #[error("entry not found for the requested target")]
    MissingEntry,
    #[error("malformed record: {0}")]
    MalformedRecord(String),
}
