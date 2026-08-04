use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, Timestamp, Uint128};

use crate::state::RoundStatus;

#[cw_serde]
pub struct InstantiateMsg {
    /// Can change config. Cannot influence any round outcome.
    pub admin: String,
    /// Native denom taken as the anti-griefing deposit, e.g. "uluna".
    pub deposit_denom: String,
    /// Minimum deposit required to open a round. Returned on reveal,
    /// forfeited to whoever calls FailRound if the secret is never revealed.
    pub min_deposit: Uint128,
    /// How long after close_time the creator still has to reveal.
    pub reveal_window_secs: u64,
    /// Upper bound on close_time - now, so nobody parks a round forever.
    pub max_round_duration_secs: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Open a round. `seed_hash` is sha256(secret) and MUST be published
    /// before the participants of the consuming event are known —
    /// that is the whole guarantee this contract offers.
    /// Attach at least `min_deposit` of `deposit_denom`.
    CreateRound {
        seed_hash: Binary,
        close_time: Timestamp,
        /// Free-form label for consumers, e.g. "oracle-draw:daily:2026-08-05".
        memo: Option<String>,
    },

    /// Add entropy to an open round. Anyone, any number of times, until
    /// close_time. Each call folds into a running accumulator, so a single
    /// honest contributor is enough to stop the creator predicting the result.
    Contribute { round_id: u64, entropy: Binary },

    /// Reveal the secret after close_time and before the reveal deadline.
    /// Callable by ANYONE who knows the secret — normally the creator.
    /// Fails unless sha256(secret) == seed_hash.
    Reveal { round_id: u64, secret: Binary },

    /// Mark a round failed once the reveal deadline has passed with no reveal.
    /// Callable by anyone; the deposit goes to the caller as a bounty.
    FailRound { round_id: u64 },

    UpdateConfig {
        admin: Option<String>,
        min_deposit: Option<Uint128>,
        reveal_window_secs: Option<u64>,
        max_round_duration_secs: Option<u64>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(ConfigResponse)]
    Config {},

    #[returns(RoundResponse)]
    Round { round_id: u64 },

    #[returns(RoundsResponse)]
    Rounds {
        start_after: Option<u64>,
        limit: Option<u32>,
    },

    /// Convenience for consumers: errors unless the round is revealed.
    #[returns(ResultResponse)]
    Result { round_id: u64 },
}

#[cw_serde]
pub struct ConfigResponse {
    pub admin: String,
    pub deposit_denom: String,
    pub min_deposit: Uint128,
    pub reveal_window_secs: u64,
    pub max_round_duration_secs: u64,
    pub next_round_id: u64,
}

#[cw_serde]
pub struct RoundResponse {
    pub round_id: u64,
    pub creator: String,
    pub seed_hash: Binary,
    pub close_time: Timestamp,
    pub reveal_deadline: Timestamp,
    pub deposit: Uint128,
    /// Running accumulator of contributed entropy. Public on purpose:
    /// contributors must be able to confirm their input was included.
    pub entropy: Binary,
    pub contributions: u32,
    pub status: RoundStatus,
    pub secret: Option<Binary>,
    pub result: Option<Binary>,
    pub memo: Option<String>,
}

#[cw_serde]
pub struct RoundsResponse {
    pub rounds: Vec<RoundResponse>,
}

#[cw_serde]
pub struct ResultResponse {
    pub round_id: u64,
    /// 32 bytes. Derive a number with the documented rule:
    /// n = u128::from_be_bytes(result[0..16]) % range
    pub result: Binary,
    pub secret: Binary,
    pub entropy: Binary,
}
