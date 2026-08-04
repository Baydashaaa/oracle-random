[README.md](https://github.com/user-attachments/files/30719307/README.md)
# oracle-random

Commit-reveal randomness beacon for Terra Classic.

A creator publishes `sha256(secret)` **before** the participants of the
consuming event exist. Anyone except the creator folds entropy into the round
until it closes. After close the secret is revealed and the result is fixed:

```
result = sha256(secret || entropy || round_id)
entropy = sha256(entropy || contributor_address || contributed_bytes)   # per call
```

Anyone can recompute the result from public chain data.

## Guarantees

- **The creator cannot pick the outcome.** The commitment predates the entries,
  and the creator is barred from contributing entropy — otherwise they could
  grind their own contribution, knowing the secret, until the result named a
  chosen winner.
- **The creator cannot re-roll.** A wrong secret is rejected. Silence past the
  deadline lets anyone call `FailRound` and take the deposit.
- **A round with no outside entropy cannot resolve.** Set `min_contributions`
  to at least 1 for anything with money on it.

## Known limitation — read this before relying on it

If the creator controls a second address and contributes from it, they regain
the ability to grind the result. The on-chain check stops the obvious case, not
a determined operator with two wallets.

Closing that gap properly requires either contributors to commit-reveal as well
(a second transaction from every participant), or an external threshold beacon
such as drand, which needs BLS12-381 host functions in the chain's CosmWasm
version. Neither is in this contract yet. Until then, treat the guarantee as:
*safe against a creator who does not sybil their own round*, and publish the
round early so the ordering is visible.

## Statuses

`Open` → `Revealed` on a valid secret. `Open` → `Failed` if nobody reveals in
time. `Open` → `Cancelled` if the round closed under `min_contributions` and
the creator reclaimed the deposit. `Closed` is derived in queries, never
stored — no transaction is needed just because time passed.

**Consumers must treat `Failed` and `Cancelled` as "no result"**, never as
permission to fall back to another source of randomness silently.

## Deriving a number

```rust
let n = oracle_random::random_in_range(&result, total_entries)?;
```

Modulo bias is below 2^-100 for any range a draw would use.

## Note on the deposit

Refunds go out as `BankMsg::Send` and therefore pay the Terra Classic burn tax.
The creator gets back slightly less than they put in — size `min_deposit`
accordingly.

## Build

```bash
cargo test
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/optimizer:0.16.0
```

Match the optimizer and the `cosmwasm-std` version to what oracle-score used
and to what Terra Classic supports.
