use cosmwasm_std::StdError;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("Round {round_id} not found")]
    RoundNotFound { round_id: u64 },

    #[error("seed_hash must be exactly 32 bytes, got {got}")]
    BadSeedHash { got: usize },

    #[error("close_time must be in the future")]
    CloseTimeInPast {},

    #[error("close_time is too far ahead (max {max_secs}s from now)")]
    CloseTimeTooFar { max_secs: u64 },

    #[error("deposit of at least {required}{denom} is required")]
    DepositTooSmall { required: String, denom: String },

    #[error("entropy must be 1..={max} bytes")]
    BadEntropy { max: usize },

    #[error("round {round_id} is closed for contributions")]
    ContributionsClosed { round_id: u64 },

    #[error("round {round_id} is not open (already revealed or failed)")]
    RoundNotOpen { round_id: u64 },

    #[error("too early to reveal: wait until close_time")]
    RevealTooEarly {},

    #[error("reveal deadline has passed; the round can only be failed now")]
    RevealTooLate {},

    #[error("secret does not match the committed seed_hash")]
    SecretMismatch {},

    #[error("reveal deadline has not passed yet")]
    NotFailedYet {},

    #[error("invalid config: {reason}")]
    InvalidConfig { reason: String },
}
