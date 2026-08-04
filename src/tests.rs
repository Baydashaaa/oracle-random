use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{coins, from_json, Binary, CosmosMsg, BankMsg, Timestamp, Uint128};

use crate::contract::{execute, instantiate, query, random_in_range, sha256};
use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, RoundResponse};
use crate::state::RoundStatus;

const DENOM: &str = "uluna";
const CREATOR: &str = "creator";
const ALICE: &str = "alice";
const BOB: &str = "bob";
const MIN_DURATION: u64 = 600;
const REVEAL_WINDOW: u64 = 3600;

fn setup() -> cosmwasm_std::OwnedDeps<
    cosmwasm_std::testing::MockStorage,
    cosmwasm_std::testing::MockApi,
    cosmwasm_std::testing::MockQuerier,
> {
    let mut deps = mock_dependencies();
    let msg = InstantiateMsg {
        admin: "admin".to_string(),
        deposit_denom: DENOM.to_string(),
        min_deposit: Uint128::new(1000),
        min_round_duration_secs: MIN_DURATION,
        max_round_duration_secs: 86400,
        reveal_window_secs: REVEAL_WINDOW,
    };
    instantiate(deps.as_mut(), mock_env(), mock_info("admin", &[]), msg).unwrap();
    deps
}

fn secret_bytes() -> Binary {
    Binary::from(b"correct horse battery staple".to_vec())
}

fn create_round(
    deps: cosmwasm_std::DepsMut,
    min_contributions: u32,
) -> Result<cosmwasm_std::Response, ContractError> {
    let env = mock_env();
    let msg = ExecuteMsg::CreateRound {
        seed_hash: sha256(secret_bytes().as_slice()),
        close_time: env.block.time.plus_seconds(MIN_DURATION),
        min_contributions,
        memo: Some("oracle-draw:daily".to_string()),
    };
    execute(deps, env, mock_info(CREATOR, &coins(1000, DENOM)), msg)
}

fn at(offset: u64) -> cosmwasm_std::Env {
    let mut env = mock_env();
    env.block.time = env.block.time.plus_seconds(offset);
    env
}

fn round(deps: cosmwasm_std::Deps, env: cosmwasm_std::Env) -> RoundResponse {
    from_json(query(deps, env, QueryMsg::Round { round_id: 1 }).unwrap()).unwrap()
}

#[test]
fn create_requires_deposit_and_sane_timing() {
    let mut deps = setup();
    let env = mock_env();
    let seed_hash = sha256(secret_bytes().as_slice());

    // too short — the commitment would not provably precede participants
    let err = execute(
        deps.as_mut(),
        env.clone(),
        mock_info(CREATOR, &coins(1000, DENOM)),
        ExecuteMsg::CreateRound {
            seed_hash: seed_hash.clone(),
            close_time: env.block.time.plus_seconds(10),
            min_contributions: 1,
            memo: None,
        },
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::RoundTooShort { .. }));

    // deposit below minimum
    let err = execute(
        deps.as_mut(),
        env.clone(),
        mock_info(CREATOR, &coins(1, DENOM)),
        ExecuteMsg::CreateRound {
            seed_hash: seed_hash.clone(),
            close_time: env.block.time.plus_seconds(MIN_DURATION),
            min_contributions: 1,
            memo: None,
        },
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::DepositTooSmall { .. }));

    // seed_hash of the wrong length
    let err = execute(
        deps.as_mut(),
        env.clone(),
        mock_info(CREATOR, &coins(1000, DENOM)),
        ExecuteMsg::CreateRound {
            seed_hash: Binary::from(b"short".to_vec()),
            close_time: env.block.time.plus_seconds(MIN_DURATION),
            min_contributions: 1,
            memo: None,
        },
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::BadSeedHash { .. }));

    create_round(deps.as_mut(), 1).unwrap();
    let r = round(deps.as_ref(), mock_env());
    assert_eq!(r.status, RoundStatus::Open);
    assert_eq!(r.contributions, 0);
    assert!(r.created_at < r.close_time);
}

#[test]
fn creator_cannot_contribute() {
    let mut deps = setup();
    create_round(deps.as_mut(), 1).unwrap();

    let err = execute(
        deps.as_mut(),
        at(10),
        mock_info(CREATOR, &[]),
        ExecuteMsg::Contribute {
            round_id: 1,
            entropy: Binary::from(b"mine".to_vec()),
        },
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::CreatorCannotContribute {}));
}

#[test]
fn contributions_close_on_time() {
    let mut deps = setup();
    create_round(deps.as_mut(), 1).unwrap();

    execute(
        deps.as_mut(),
        at(10),
        mock_info(ALICE, &[]),
        ExecuteMsg::Contribute {
            round_id: 1,
            entropy: Binary::from(b"alice".to_vec()),
        },
    )
    .unwrap();

    let err = execute(
        deps.as_mut(),
        at(MIN_DURATION),
        mock_info(BOB, &[]),
        ExecuteMsg::Contribute {
            round_id: 1,
            entropy: Binary::from(b"late".to_vec()),
        },
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::ContributionsClosed { .. }));
}

#[test]
fn reveal_rules() {
    let mut deps = setup();
    create_round(deps.as_mut(), 1).unwrap();

    // no contributions yet, and it is too early anyway
    let err = execute(
        deps.as_mut(),
        at(10),
        mock_info(CREATOR, &[]),
        ExecuteMsg::Reveal {
            round_id: 1,
            secret: secret_bytes(),
        },
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::RevealTooEarly {}));

    // closed, but nobody contributed -> cannot reveal at all
    let err = execute(
        deps.as_mut(),
        at(MIN_DURATION + 1),
        mock_info(CREATOR, &[]),
        ExecuteMsg::Reveal {
            round_id: 1,
            secret: secret_bytes(),
        },
    )
    .unwrap_err();
    assert!(matches!(
        err,
        ContractError::NotEnoughContributions { have: 0, need: 1 }
    ));

    // add a contribution in a fresh round and reveal with a wrong secret
    let mut deps = setup();
    create_round(deps.as_mut(), 1).unwrap();
    execute(
        deps.as_mut(),
        at(10),
        mock_info(ALICE, &[]),
        ExecuteMsg::Contribute {
            round_id: 1,
            entropy: Binary::from(b"alice".to_vec()),
        },
    )
    .unwrap();

    let err = execute(
        deps.as_mut(),
        at(MIN_DURATION + 1),
        mock_info(CREATOR, &[]),
        ExecuteMsg::Reveal {
            round_id: 1,
            secret: Binary::from(b"wrong".to_vec()),
        },
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::SecretMismatch {}));

    // a stranger may reveal, but the deposit goes back to the creator
    let res = execute(
        deps.as_mut(),
        at(MIN_DURATION + 1),
        mock_info(BOB, &[]),
        ExecuteMsg::Reveal {
            round_id: 1,
            secret: secret_bytes(),
        },
    )
    .unwrap();
    match &res.messages[0].msg {
        CosmosMsg::Bank(BankMsg::Send { to_address, amount }) => {
            assert_eq!(to_address, CREATOR);
            assert_eq!(amount[0].amount, Uint128::new(1000));
        }
        other => panic!("unexpected message: {other:?}"),
    }

    let r = round(deps.as_ref(), at(MIN_DURATION + 1));
    assert_eq!(r.status, RoundStatus::Revealed);
    assert!(r.result.is_some());

    // and it cannot be revealed twice
    let err = execute(
        deps.as_mut(),
        at(MIN_DURATION + 2),
        mock_info(CREATOR, &[]),
        ExecuteMsg::Reveal {
            round_id: 1,
            secret: secret_bytes(),
        },
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::RoundNotOpen { .. }));
}

/// The point of the whole contract: the same secret must produce a different
/// result once someone else has folded entropy in. If this test ever fails,
/// the creator can predict outcomes.
#[test]
fn contributions_change_the_result() {
    fn result_with(contributors: &[&str]) -> Binary {
        let mut deps = setup();
        create_round(deps.as_mut(), 1).unwrap();
        for (i, who) in contributors.iter().enumerate() {
            execute(
                deps.as_mut(),
                at(10 + i as u64),
                mock_info(who, &[]),
                ExecuteMsg::Contribute {
                    round_id: 1,
                    entropy: Binary::from(b"same-bytes".to_vec()),
                },
            )
            .unwrap();
        }
        execute(
            deps.as_mut(),
            at(MIN_DURATION + 1),
            mock_info(CREATOR, &[]),
            ExecuteMsg::Reveal {
                round_id: 1,
                secret: secret_bytes(),
            },
        )
        .unwrap();
        round(deps.as_ref(), at(MIN_DURATION + 1)).result.unwrap()
    }

    let one = result_with(&[ALICE]);
    let two = result_with(&[ALICE, BOB]);
    let swapped = result_with(&[BOB, ALICE]);
    let repeat = result_with(&[ALICE]);

    assert_ne!(one, two, "an extra contributor must move the result");
    assert_ne!(two, swapped, "contribution order must matter");
    assert_eq!(one, repeat, "same inputs must be reproducible");
}

#[test]
fn fail_round_pays_the_caller() {
    let mut deps = setup();
    create_round(deps.as_mut(), 1).unwrap();
    execute(
        deps.as_mut(),
        at(10),
        mock_info(ALICE, &[]),
        ExecuteMsg::Contribute {
            round_id: 1,
            entropy: Binary::from(b"alice".to_vec()),
        },
    )
    .unwrap();

    let err = execute(
        deps.as_mut(),
        at(MIN_DURATION + 1),
        mock_info(BOB, &[]),
        ExecuteMsg::FailRound { round_id: 1 },
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::NotFailedYet {}));

    let res = execute(
        deps.as_mut(),
        at(MIN_DURATION + REVEAL_WINDOW + 1),
        mock_info(BOB, &[]),
        ExecuteMsg::FailRound { round_id: 1 },
    )
    .unwrap();
    match &res.messages[0].msg {
        CosmosMsg::Bank(BankMsg::Send { to_address, .. }) => assert_eq!(to_address, BOB),
        other => panic!("unexpected message: {other:?}"),
    }
    assert_eq!(
        round(deps.as_ref(), at(MIN_DURATION + REVEAL_WINDOW + 2)).status,
        RoundStatus::Failed
    );
}

#[test]
fn cancel_only_when_under_subscribed() {
    let mut deps = setup();
    create_round(deps.as_mut(), 2).unwrap();
    execute(
        deps.as_mut(),
        at(10),
        mock_info(ALICE, &[]),
        ExecuteMsg::Contribute {
            round_id: 1,
            entropy: Binary::from(b"alice".to_vec()),
        },
    )
    .unwrap();

    // not yet closed
    let err = execute(
        deps.as_mut(),
        at(10),
        mock_info(CREATOR, &[]),
        ExecuteMsg::CancelRound { round_id: 1 },
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::NotCancellable {}));

    // not the creator
    let err = execute(
        deps.as_mut(),
        at(MIN_DURATION + 1),
        mock_info(BOB, &[]),
        ExecuteMsg::CancelRound { round_id: 1 },
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::Unauthorized {}));

    // under-subscribed and closed -> refund
    let res = execute(
        deps.as_mut(),
        at(MIN_DURATION + 1),
        mock_info(CREATOR, &[]),
        ExecuteMsg::CancelRound { round_id: 1 },
    )
    .unwrap();
    match &res.messages[0].msg {
        CosmosMsg::Bank(BankMsg::Send { to_address, .. }) => assert_eq!(to_address, CREATOR),
        other => panic!("unexpected message: {other:?}"),
    }

    // a round that met its threshold cannot be cancelled
    let mut deps = setup();
    create_round(deps.as_mut(), 1).unwrap();
    execute(
        deps.as_mut(),
        at(10),
        mock_info(ALICE, &[]),
        ExecuteMsg::Contribute {
            round_id: 1,
            entropy: Binary::from(b"alice".to_vec()),
        },
    )
    .unwrap();
    let err = execute(
        deps.as_mut(),
        at(MIN_DURATION + 1),
        mock_info(CREATOR, &[]),
        ExecuteMsg::CancelRound { round_id: 1 },
    )
    .unwrap_err();
    assert!(matches!(err, ContractError::NotCancellable {}));
}

#[test]
fn derived_status_and_range_helper() {
    let mut deps = setup();
    create_round(deps.as_mut(), 1).unwrap();

    assert_eq!(round(deps.as_ref(), at(10)).status, RoundStatus::Open);
    assert_eq!(
        round(deps.as_ref(), at(MIN_DURATION + 1)).status,
        RoundStatus::Closed
    );

    let r = Binary::from(vec![0xffu8; 32]);
    assert!(random_in_range(&r, 7).unwrap() < 7);
    assert!(random_in_range(&r, 0).is_err());
    assert!(random_in_range(&Binary::from(vec![1u8; 8]), 7).is_err());
    let _ = Timestamp::from_seconds(0);
}
