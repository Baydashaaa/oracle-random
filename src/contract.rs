#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Addr, BankMsg, Binary, Coin, Deps, DepsMut, Env, MessageInfo, Order, Response,
    StdError, StdResult, Timestamp, Uint128,
};
use cw_storage_plus::Bound;
use sha2::{Digest, Sha256};

use crate::error::ContractError;
use crate::msg::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, ResultResponse, RoundResponse,
    RoundsResponse,
};
use crate::state::{Config, Round, RoundStatus, CONFIG, NEXT_ROUND_ID, ROUNDS};

const CONTRACT_NAME: &str = "crates.io:oracle-random";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Cap on a single entropy contribution. Anything longer is spam:
/// it is hashed anyway, so extra bytes add no unpredictability.
const MAX_ENTROPY_BYTES: usize = 256;

const DEFAULT_LIMIT: u32 = 30;
const MAX_LIMIT: u32 = 100;

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    if msg.reveal_window_secs == 0 {
        return Err(ContractError::InvalidConfig {
            reason: "reveal_window_secs must be > 0".into(),
        });
    }
    if msg.max_round_duration_secs == 0 {
        return Err(ContractError::InvalidConfig {
            reason: "max_round_duration_secs must be > 0".into(),
        });
    }
    if msg.deposit_denom.is_empty() {
        return Err(ContractError::InvalidConfig {
            reason: "deposit_denom must not be empty".into(),
        });
    }

    let config = Config {
        admin: deps.api.addr_validate(&msg.admin)?,
        deposit_denom: msg.deposit_denom,
        min_deposit: msg.min_deposit,
        reveal_window_secs: msg.reveal_window_secs,
        max_round_duration_secs: msg.max_round_duration_secs,
    };
    CONFIG.save(deps.storage, &config)?;
    NEXT_ROUND_ID.save(deps.storage, &1u64)?;

    Ok(Response::new()
        .add_attribute("action", "instantiate")
        .add_attribute("admin", config.admin))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::CreateRound {
            seed_hash,
            close_time,
            memo,
        } => execute_create_round(deps, env, info, seed_hash, close_time, memo),
        ExecuteMsg::Contribute { round_id, entropy } => {
            execute_contribute(deps, env, info, round_id, entropy)
        }
        ExecuteMsg::Reveal { round_id, secret } => execute_reveal(deps, env, round_id, secret),
        ExecuteMsg::FailRound { round_id } => execute_fail_round(deps, env, info, round_id),
        ExecuteMsg::UpdateConfig {
            admin,
            min_deposit,
            reveal_window_secs,
            max_round_duration_secs,
        } => execute_update_config(
            deps,
            info,
            admin,
            min_deposit,
            reveal_window_secs,
            max_round_duration_secs,
        ),
    }
}

fn execute_create_round(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    seed_hash: Binary,
    close_time: Timestamp,
    memo: Option<String>,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;

    if seed_hash.len() != 32 {
        return Err(ContractError::BadSeedHash {
            got: seed_hash.len(),
        });
    }
    if close_time <= env.block.time {
        return Err(ContractError::CloseTimeInPast {});
    }
    let max_close = env.block.time.plus_seconds(config.max_round_duration_secs);
    if close_time > max_close {
        return Err(ContractError::CloseTimeTooFar {
            max_secs: config.max_round_duration_secs,
        });
    }

    let deposit = funds_of(&info.funds, &config.deposit_denom);
    if deposit < config.min_deposit {
        return Err(ContractError::DepositTooSmall {
            required: config.min_deposit.to_string(),
            denom: config.deposit_denom,
        });
    }

    let round_id = NEXT_ROUND_ID.load(deps.storage)?;

    // Seed the accumulator so it is never empty even with zero contributions.
    let mut hasher = Sha256::new();
    hasher.update(round_id.to_be_bytes());
    hasher.update(info.sender.as_bytes());
    hasher.update(seed_hash.as_slice());
    let entropy = Binary::from(hasher.finalize().as_slice());

    let round = Round {
        creator: info.sender.clone(),
        seed_hash,
        close_time,
        reveal_deadline: close_time.plus_seconds(config.reveal_window_secs),
        deposit,
        entropy,
        contributions: 0,
        status: RoundStatus::Open,
        secret: None,
        result: None,
        memo,
    };

    ROUNDS.save(deps.storage, round_id, &round)?;
    NEXT_ROUND_ID.save(deps.storage, &(round_id + 1))?;

    Ok(Response::new()
        .add_attribute("action", "create_round")
        .add_attribute("round_id", round_id.to_string())
        .add_attribute("creator", info.sender)
        .add_attribute("close_time", close_time.seconds().to_string())
        .add_attribute("reveal_deadline", round.reveal_deadline.seconds().to_string())
        .add_attribute("deposit", deposit))
}

fn execute_contribute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    round_id: u64,
    entropy: Binary,
) -> Result<Response, ContractError> {
    if entropy.is_empty() || entropy.len() > MAX_ENTROPY_BYTES {
        return Err(ContractError::BadEntropy {
            max: MAX_ENTROPY_BYTES,
        });
    }

    let mut round = load_round(deps.as_ref(), round_id)?;

    if round.status != RoundStatus::Open {
        return Err(ContractError::RoundNotOpen { round_id });
    }
    if env.block.time >= round.close_time {
        return Err(ContractError::ContributionsClosed { round_id });
    }

    let mut hasher = Sha256::new();
    hasher.update(round.entropy.as_slice());
    hasher.update(info.sender.as_bytes());
    hasher.update(entropy.as_slice());
    round.entropy = Binary::from(hasher.finalize().as_slice());
    round.contributions += 1;

    ROUNDS.save(deps.storage, round_id, &round)?;

    Ok(Response::new()
        .add_attribute("action", "contribute")
        .add_attribute("round_id", round_id.to_string())
        .add_attribute("contributor", info.sender)
        .add_attribute("contributions", round.contributions.to_string())
        .add_attribute("entropy", round.entropy.to_base64()))
}

/// Callable by anyone holding the secret — normally the creator.
/// The deposit always goes back to the creator, never to the caller,
/// so there is nothing to gain by front-running someone else's reveal.
fn execute_reveal(
    deps: DepsMut,
    env: Env,
    round_id: u64,
    secret: Binary,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let mut round = load_round(deps.as_ref(), round_id)?;

    if round.status != RoundStatus::Open {
        return Err(ContractError::RoundNotOpen { round_id });
    }
    if env.block.time < round.close_time {
        return Err(ContractError::RevealTooEarly {});
    }
    if env.block.time > round.reveal_deadline {
        return Err(ContractError::RevealTooLate {});
    }

    let mut hasher = Sha256::new();
    hasher.update(secret.as_slice());
    let computed = hasher.finalize();
    if computed.as_slice() != round.seed_hash.as_slice() {
        return Err(ContractError::SecretMismatch {});
    }

    // result = sha256(secret || entropy || round_id)
    let mut hasher = Sha256::new();
    hasher.update(secret.as_slice());
    hasher.update(round.entropy.as_slice());
    hasher.update(round_id.to_be_bytes());
    let result = Binary::from(hasher.finalize().as_slice());

    round.status = RoundStatus::Revealed;
    round.secret = Some(secret);
    round.result = Some(result.clone());
    ROUNDS.save(deps.storage, round_id, &round)?;

    let mut res = Response::new()
        .add_attribute("action", "reveal")
        .add_attribute("round_id", round_id.to_string())
        .add_attribute("result", result.to_base64())
        .add_attribute("contributions", round.contributions.to_string());

    if !round.deposit.is_zero() {
        res = res.add_message(BankMsg::Send {
            to_address: round.creator.to_string(),
            amount: vec![Coin {
                denom: config.deposit_denom,
                amount: round.deposit,
            }],
        });
    }

    Ok(res)
}

fn execute_fail_round(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    round_id: u64,
) -> Result<Response, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    let mut round = load_round(deps.as_ref(), round_id)?;

    if round.status != RoundStatus::Open {
        return Err(ContractError::RoundNotOpen { round_id });
    }
    if env.block.time <= round.reveal_deadline {
        return Err(ContractError::NotFailedYet {});
    }

    round.status = RoundStatus::Failed;
    ROUNDS.save(deps.storage, round_id, &round)?;

    let mut res = Response::new()
        .add_attribute("action", "fail_round")
        .add_attribute("round_id", round_id.to_string())
        .add_attribute("bounty_to", info.sender.to_string());

    if !round.deposit.is_zero() {
        res = res.add_message(BankMsg::Send {
            to_address: info.sender.to_string(),
            amount: vec![Coin {
                denom: config.deposit_denom,
                amount: round.deposit,
            }],
        });
    }

    Ok(res)
}

fn execute_update_config(
    deps: DepsMut,
    info: MessageInfo,
    admin: Option<String>,
    min_deposit: Option<Uint128>,
    reveal_window_secs: Option<u64>,
    max_round_duration_secs: Option<u64>,
) -> Result<Response, ContractError> {
    let mut config = CONFIG.load(deps.storage)?;
    if info.sender != config.admin {
        return Err(ContractError::Unauthorized {});
    }

    if let Some(admin) = admin {
        config.admin = deps.api.addr_validate(&admin)?;
    }
    if let Some(min_deposit) = min_deposit {
        config.min_deposit = min_deposit;
    }
    if let Some(secs) = reveal_window_secs {
        if secs == 0 {
            return Err(ContractError::InvalidConfig {
                reason: "reveal_window_secs must be > 0".into(),
            });
        }
        config.reveal_window_secs = secs;
    }
    if let Some(secs) = max_round_duration_secs {
        if secs == 0 {
            return Err(ContractError::InvalidConfig {
                reason: "max_round_duration_secs must be > 0".into(),
            });
        }
        config.max_round_duration_secs = secs;
    }

    CONFIG.save(deps.storage, &config)?;

    Ok(Response::new()
        .add_attribute("action", "update_config")
        .add_attribute("admin", config.admin))
}

fn funds_of(funds: &[Coin], denom: &str) -> Uint128 {
    funds
        .iter()
        .filter(|c| c.denom == denom)
        .fold(Uint128::zero(), |acc, c| acc + c.amount)
}

fn load_round(deps: Deps, round_id: u64) -> Result<Round, ContractError> {
    ROUNDS
        .may_load(deps.storage, round_id)?
        .ok_or(ContractError::RoundNotFound { round_id })
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => to_json_binary(&query_config(deps)?),
        QueryMsg::Round { round_id } => to_json_binary(&query_round(deps, env, round_id)?),
        QueryMsg::Rounds { start_after, limit } => {
            to_json_binary(&query_rounds(deps, env, start_after, limit)?)
        }
        QueryMsg::Result { round_id } => to_json_binary(&query_result(deps, round_id)?),
    }
}

fn query_config(deps: Deps) -> StdResult<ConfigResponse> {
    let config = CONFIG.load(deps.storage)?;
    Ok(ConfigResponse {
        admin: config.admin.to_string(),
        deposit_denom: config.deposit_denom,
        min_deposit: config.min_deposit,
        reveal_window_secs: config.reveal_window_secs,
        max_round_duration_secs: config.max_round_duration_secs,
        next_round_id: NEXT_ROUND_ID.load(deps.storage)?,
    })
}

/// Storage only ever holds Open / Revealed / Failed. `Closed` is derived here:
/// a round past close_time but not yet resolved has stopped taking entropy
/// and is waiting for the secret. Nothing writes Closed to storage, so no
/// transaction is needed just to move a round forward in time.
fn derived_status(round: &Round, now: Timestamp) -> RoundStatus {
    match round.status {
        RoundStatus::Open if now >= round.close_time => RoundStatus::Closed,
        ref other => other.clone(),
    }
}

fn round_to_response(round_id: u64, round: Round, now: Timestamp) -> RoundResponse {
    let status = derived_status(&round, now);
    RoundResponse {
        round_id,
        creator: round.creator.to_string(),
        seed_hash: round.seed_hash,
        close_time: round.close_time,
        reveal_deadline: round.reveal_deadline,
        deposit: round.deposit,
        entropy: round.entropy,
        contributions: round.contributions,
        status,
        secret: round.secret,
        result: round.result,
        memo: round.memo,
    }
}

fn query_round(deps: Deps, env: Env, round_id: u64) -> StdResult<RoundResponse> {
    let round = ROUNDS
        .may_load(deps.storage, round_id)?
        .ok_or_else(|| StdError::not_found(format!("round {round_id}")))?;
    Ok(round_to_response(round_id, round, env.block.time))
}

fn query_rounds(
    deps: Deps,
    env: Env,
    start_after: Option<u64>,
    limit: Option<u32>,
) -> StdResult<RoundsResponse> {
    let limit = limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT) as usize;
    let start = start_after.map(Bound::exclusive);

    let rounds = ROUNDS
        .range(deps.storage, start, None, Order::Ascending)
        .take(limit)
        .map(|item| {
            let (round_id, round) = item?;
            Ok(round_to_response(round_id, round, env.block.time))
        })
        .collect::<StdResult<Vec<_>>>()?;

    Ok(RoundsResponse { rounds })
}

fn query_result(deps: Deps, round_id: u64) -> StdResult<ResultResponse> {
    let round = ROUNDS
        .may_load(deps.storage, round_id)?
        .ok_or_else(|| StdError::not_found(format!("round {round_id}")))?;

    match (round.result, round.secret) {
        (Some(result), Some(secret)) => Ok(ResultResponse {
            round_id,
            result,
            secret,
            entropy: round.entropy,
        }),
        _ => Err(StdError::generic_err(format!(
            "round {round_id} has no result: status is {:?}",
            round.status
        ))),
    }
}

/// Helper for consumers: turn a 32-byte result into 0..range-1.
/// Modulo bias is below 2^-100 for any range a lottery would ever use.
pub fn random_in_range(result: &Binary, range: u128) -> StdResult<u128> {
    if range == 0 {
        return Err(StdError::generic_err("range must be > 0"));
    }
    if result.len() < 16 {
        return Err(StdError::generic_err("result must be at least 16 bytes"));
    }
    let mut buf = [0u8; 16];
    buf.copy_from_slice(&result.as_slice()[0..16]);
    Ok(u128::from_be_bytes(buf) % range)
}

/// Convenience for off-chain code and consumers building a seed commitment.
pub fn sha256(bytes: &[u8]) -> Binary {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Binary::from(hasher.finalize().as_slice())
}

#[allow(dead_code)]
fn _assert_addr_type(_: &Addr) {}
