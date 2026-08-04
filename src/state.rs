use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary, Timestamp, Uint128};
use cw_storage_plus::{Item, Map};

/// WARNING: every field here lands in storage. Changing or reordering a field
/// after the first mainnet deploy makes existing rounds unreadable and forces a
/// data migration — and unlike oracle-score, these rounds may belong to other
/// people's contracts. Get this struct right before the first mainnet store.
#[cw_serde]
pub struct Config {
    /// Can change config. Cannot influence any round outcome.
    pub admin: Addr,
    /// Native denom taken as the anti-griefing deposit, e.g. "uluna".
    pub deposit_denom: String,
    /// Returned on reveal or cancel; forfeited to whoever calls FailRound.
    pub min_deposit: Uint128,
    /// Minimum gap between round creation and close_time. This is what makes
    /// "the commitment predates the participants" checkable on-chain rather
    /// than merely claimed.
    pub min_round_duration_secs: u64,
    /// Upper bound on close_time - now, so nobody parks a round forever.
    pub max_round_duration_secs: u64,
    /// How long after close_time the secret may still be revealed.
    pub reveal_window_secs: u64,
}

#[cw_serde]
pub enum RoundStatus {
    /// Accepting entropy, secret not yet revealed.
    Open,
    /// Past close_time, waiting for the secret.
    /// NEVER written to storage — derived in queries from close_time, so that
    /// moving a round forward in time costs no transaction.
    Closed,
    /// Secret revealed, result final and immutable.
    Revealed,
    /// Reveal deadline passed with no valid secret. The deposit went to the
    /// caller of FailRound. Consumers MUST treat this as "no result" and
    /// never as permission to roll again silently.
    Failed,
    /// Closed without reaching min_contributions; creator reclaimed the
    /// deposit. Also "no result".
    Cancelled,
}

#[cw_serde]
pub struct Round {
    pub creator: Addr,
    /// sha256(secret), 32 bytes, fixed at creation.
    pub seed_hash: Binary,
    /// Block time at creation. Together with close_time this proves the
    /// commitment came first.
    pub created_at: Timestamp,
    pub close_time: Timestamp,
    pub reveal_deadline: Timestamp,
    pub deposit: Uint128,
    /// entropy = sha256(entropy || sender || contributed_bytes), folded per
    /// call. Seeded at creation so it is never empty.
    pub entropy: Binary,
    pub contributions: u32,
    /// The round cannot be revealed with fewer contributions than this.
    /// Set it above zero for anything with money on it: with no outside
    /// entropy the result follows from the creator's secret alone.
    pub min_contributions: u32,
    pub status: RoundStatus,
    pub secret: Option<Binary>,
    pub result: Option<Binary>,
    pub memo: Option<String>,
}

pub const CONFIG: Item<Config> = Item::new("config");
pub const NEXT_ROUND_ID: Item<u64> = Item::new("next_round_id");
pub const ROUNDS: Map<u64, Round> = Map::new("rounds");
