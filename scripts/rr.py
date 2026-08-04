#!/usr/bin/env python3
"""Helpers for exercising oracle-random by hand.

Two things this exists to get right:
  * CosmWasm Timestamp is NANOSECONDS as a JSON string, not seconds
  * CosmWasm Binary is BASE64 in JSON, not hex
Both fail silently if you get them wrong.
"""
import base64
import hashlib
import json
import subprocess
import sys
import time


def sha256(*parts: bytes) -> bytes:
    h = hashlib.sha256()
    for p in parts:
        h.update(p)
    return h.digest()


def b64(raw: bytes) -> str:
    return base64.b64encode(raw).decode()


def unb64(s: str) -> bytes:
    return base64.b64decode(s)


def cmd_gen(argv):
    """gen <seconds_from_now> -> secret + ready-to-send CreateRound msg"""
    offset = int(argv[0]) if argv else 600
    secret = hashlib.sha256(str(time.time_ns()).encode() + b"oracle-random").digest()
    seed_hash = sha256(secret)
    close_ns = (int(time.time()) + offset) * 1_000_000_000

    print("SAVE THIS SECRET — without it the round can never be revealed:")
    print("  secret_hex   :", secret.hex())
    print("  secret_base64:", b64(secret))
    print()
    print("CreateRound msg:")
    print(json.dumps({
        "create_round": {
            "seed_hash": b64(seed_hash),
            "close_time": str(close_ns),
            "min_contributions": 1,
            "memo": "rehearsal",
        }
    }))
    print()
    print("Reveal msg (after close_time):")
    print(json.dumps({"reveal": {"round_id": 1, "secret": b64(secret)}}))


def cmd_entropy(argv):
    """entropy <text> -> Contribute msg"""
    round_id = int(argv[0])
    text = argv[1].encode()
    print(json.dumps({
        "contribute": {"round_id": round_id, "entropy": b64(text)}
    }))


def cmd_expect(argv):
    """expect <round_id> <creator> <secret_hex> [<sender>:<text> ...]

    Recomputes the whole chain independently of the contract:
      entropy_0 = sha256(round_id_be8 || creator || seed_hash)
      entropy_n = sha256(entropy_{n-1} || sender || bytes)
      result    = sha256(secret || entropy || round_id_be8)
    """
    round_id = int(argv[0])
    creator = argv[1]
    secret = bytes.fromhex(argv[2])
    seed_hash = sha256(secret)

    rid = round_id.to_bytes(8, "big")
    entropy = sha256(rid, creator.encode(), seed_hash)
    for item in argv[3:]:
        sender, _, text = item.partition(":")
        entropy = sha256(entropy, sender.encode(), text.encode())

    result = sha256(secret, entropy, rid)
    print("expected entropy:", b64(entropy))
    print("expected result :", b64(result))
    print("as number mod 10:", int.from_bytes(result[:16], "big") % 10)


def cmd_verify(argv):
    """verify <contract_addr> <round_id> <secret_hex> -- queries the chain"""
    addr, round_id, secret_hex = argv[0], int(argv[1]), argv[2]
    query = json.dumps({"round": {"round_id": round_id}})
    out = subprocess.run(
        ["terrad", "q", "wasm", "contract-state", "smart", addr, query, "-o", "json"],
        capture_output=True, text=True,
    )
    if out.returncode != 0:
        sys.exit("query failed: " + out.stderr[:400])
    data = json.loads(out.stdout)["data"]

    print("status       :", data["status"])
    print("contributions:", data["contributions"])
    print("entropy      :", data["entropy"])
    print("result       :", data.get("result"))

    if not data.get("result"):
        sys.exit("round has no result yet")

    secret = bytes.fromhex(secret_hex)
    rid = round_id.to_bytes(8, "big")
    recomputed = sha256(secret, unb64(data["entropy"]), rid)

    print()
    print("recomputed   :", b64(recomputed))
    if b64(recomputed) == data["result"]:
        print("MATCH — the contract computed what the formula says")
    else:
        sys.exit("MISMATCH — do not deploy this to mainnet")


COMMANDS = {
    "gen": cmd_gen,
    "entropy": cmd_entropy,
    "expect": cmd_expect,
    "verify": cmd_verify,
}

if __name__ == "__main__":
    if len(sys.argv) < 2 or sys.argv[1] not in COMMANDS:
        print(__doc__)
        for name, fn in COMMANDS.items():
            print(f"  {name:8} {fn.__doc__.splitlines()[0]}")
        sys.exit(1)
    COMMANDS[sys.argv[1]](sys.argv[2:])
