use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Binary, Timestamp, Uint128};

use crate::state::RoundStatus;

#[cw_serde]
pub struct InstantiateMsg {
    pub admin: String,
    pub deposit_denom: String,
    pub min_deposit: Uint128,
    pub min_round_duration_secs: u64,
    pub max_round_duration_secs: u64,
    pub reveal_window_secs: u64,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Open a round. `seed_hash` is sha256(secret) and is fixed forever here.
    /// Attach at least `min_deposit` of `deposit_denom`.
    ///
    /// The guarantee this contract provides is: the creator committed before
    /// the participants of the consuming event existed. Publish the round
    /// first, collect entries afterwards — `min_round_duration_secs` makes
    /// that order verifiable.
    CreateRound {
        seed_hash: Binary,
        close_time: Timestamp,
        /// Reveal is refused below this many contributions. Use >= 1 whenever
        /// the outcome is worth money.
        min_contributions: u32,
        /// Free-form label for consumers, e.g. "oracle-draw:daily:2026-08-05".
        memo: Option<String>,
    },

    /// Fold entropy into an open round. Anyone except the creator, any number
    /// of times, until close_time.
    ///
    /// The creator is barred on purpose: they know the secret, so if they
    /// could also place the last contribution they could grind it until the
    /// result named whoever they liked.
    Contribute { round_id: u64, entropy: Binary },

    /// Reveal the secret after close_time and before the reveal deadline.
    /// Callable by ANYONE holding the secret — the deposit still returns to
    /// the creator, so there is nothing to win by front-running a reveal.
    Reveal { round_id: u64, secret: Binary },

    /// After the reveal deadline passes with no valid secret: mark the round
    /// failed and take the deposit as a bounty. Callable by anyone.
    FailRound { round_id: u64 },

    /// Creator-only, after close_time, only while contributions are below
    /// min_contributions. Refunds the deposit. A round that reached its
    /// threshold can never be cancelled.
    CancelRound { round_id: u64 },

    UpdateConfig {
        admin: Option<String>,
        min_deposit: Option<Uint128>,
        min_round_duration_secs: Option<u64>,
        max_round_duration_secs: Option<u64>,
        reveal_window_secs: Option<u64>,
    },
}

#[cw_serde]
pub struct MigrateMsg {}

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
    pub min_round_duration_secs: u64,
    pub max_round_duration_secs: u64,
    pub reveal_window_secs: u64,
    pub next_round_id: u64,
}

#[cw_serde]
pub struct RoundResponse {
    pub round_id: u64,
    pub creator: String,
    pub seed_hash: Binary,
    pub created_at: Timestamp,
    pub close_time: Timestamp,
    pub reveal_deadline: Timestamp,
    pub deposit: Uint128,
    /// Running accumulator. Public on purpose: a contributor must be able to
    /// confirm their input was folded in.
    pub entropy: Binary,
    pub contributions: u32,
    pub min_contributions: u32,
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
    pub contributions: u32,
}
