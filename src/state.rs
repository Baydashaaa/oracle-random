use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};

/// WARNING: every field here lands in storage. Changing or reordering a field
/// after mainnet deploy makes existing rounds unreadable and forces a data
/// migration — and unlike oracle-score, these rounds may belong to other
/// people's contracts. Get this struct right before the first mainnet store.
#[cw_serde]
pub struct Config {
    pub admin: Addr,
    pub deposit_denom: String,
    pub min_deposit: Uint128,
    pub reveal_window_secs: u64,
    pub max_round_duration_secs: u64,
}

#[cw_serde]
pub enum RoundStatus {
    /// Accepting entropy; not yet past close_time.
    Open,
    /// Past close_time, waiting for the secret.
    /// NEVER written to storage — derived in queries from close_time, so that
    /// moving a round forward in time costs no transaction.
    Closed,
    /// Secret revealed, result final and immutable.
    Revealed,
    /// Reveal deadline passed with no valid secret. Deposit was paid out
    /// to the caller of FailRound. Consumers MUST treat this as "no result",
    /// never as "roll again silently".
    Failed,
}

#[cw_serde]
pub struct Round {
    pub creator: Addr,
    /// sha256(secret), 32 bytes, fixed at creation.
    pub seed_hash: Binary,
    pub close_time: Timestamp,
    pub reveal_deadline: Timestamp,
    pub deposit: Uint128,
    /// entropy = sha256(entropy || sender || contributed_bytes), folded per call.
    /// Starts as sha256(round_id || creator || seed_hash) so it is never empty.
    pub entropy: Binary,
    pub contributions: u32,
    pub status: RoundStatus,
    pub secret: Option<Binary>,
    pub result: Option<Binary>,
    pub memo: Option<String>,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const NEXT_ROUND_ID: Item<u64> = Item::new("next_round_id");
pub const ROUNDS: Map<u64, Round> = Map::new("rounds");
