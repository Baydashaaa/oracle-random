# oracle-random

Commit-reveal randomness beacon for Terra Classic.

A round creator publishes `sha256(secret)` **before** the participants of the
consuming event exist. Anyone may fold extra entropy into the round until it
closes. After close the secret is revealed and the result is fixed:

```
result = sha256(secret || entropy || round_id)
```

Guarantees:

- the creator cannot pick the outcome — the commitment predates the participants
- the creator cannot re-roll — a wrong secret is rejected, silence marks the round Failed
- anyone can recompute the result from public chain data

Not a substitute for a threshold beacon such as drand: it assumes the creator
and at least one entropy contributor are not colluding.

## Build

```bash
cargo test
docker run --rm -v "$(pwd)":/code \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/optimizer:0.16.0
```

Match the optimizer version to the one used for oracle-score.
