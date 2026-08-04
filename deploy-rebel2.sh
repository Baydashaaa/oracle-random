#!/usr/bin/env bash
set -euo pipefail

KEY=oracle-dev
GAS="--gas auto --gas-adjustment 1.6 --gas-prices 28.325uluna -y"
EXPECTED=c9cb73d1c9a493a3e637eb67999c9b17c972694fca5f55664dbdf76a6d26f96b

ADMIN=$(terrad keys show "$KEY" -a)
echo "==> admin/deployer: $ADMIN"

# init-rebel2.json ships with a placeholder so nobody deploys someone else's admin
python3 - "$ADMIN" <<'PY'
import json, sys
p = "init-rebel2.json"
d = json.load(open(p))
if d["admin"] == "ADMIN_ADDRESS_HERE":
    d["admin"] = sys.argv[1]
    json.dump(d, open(p, "w"), indent=2)
    print("    filled admin in init-rebel2.json")
PY

wait_tx () {
  for _ in $(seq 1 30); do
    if out=$(terrad q tx "$1" -o json 2>/dev/null); then echo "$out"; return 0; fi
    sleep 3
  done
  echo "timeout waiting for tx $1" >&2; return 1
}

echo "==> store code"
h=$(terrad tx wasm store artifacts/oracle_random.wasm --from $KEY $GAS -o json \
  | python3 -c "import json,sys; print(json.load(sys.stdin)['txhash'])")
echo "    txhash: $h"

code_id=$(wait_tx "$h" | python3 -c "
import json,sys
d=json.load(sys.stdin)
if d.get('code',0)!=0: sys.exit('STORE FAILED: '+d.get('raw_log','')[:300])
for e in d.get('events',[]):
    if e['type']=='store_code':
        for a in e['attributes']:
            if a['key']=='code_id': print(a['value'])
")
echo "    code_id: $code_id"

echo "==> verify hash"
terrad q wasm code-info "$code_id" -o json | python3 -c "
import json,sys,base64
d=json.load(sys.stdin)
def find(o):
    if isinstance(o,dict):
        for k,v in o.items():
            if ('hash' in k.lower() or k=='checksum') and isinstance(v,str): return v
            r=find(v)
            if r: return r
    elif isinstance(o,list):
        for v in o:
            r=find(v)
            if r: return r
h=find(d)
on = base64.b64decode(h).hex() if h and h.endswith('=') else (h or '').lower()
print('    on-chain:', on)
print('    local:   ', '$EXPECTED')
sys.exit(0 if on=='$EXPECTED' else 'HASH MISMATCH')
"

echo "==> instantiate"
h=$(terrad tx wasm instantiate "$code_id" "$(cat init-rebel2.json)" \
  --label "oracle-random v0.1.0" --admin "$ADMIN" --from $KEY $GAS -o json \
  | python3 -c "import json,sys; print(json.load(sys.stdin)['txhash'])")

addr=$(wait_tx "$h" | python3 -c "
import json,sys
d=json.load(sys.stdin)
if d.get('code',0)!=0: sys.exit('INSTANTIATE FAILED: '+d.get('raw_log','')[:300])
for e in d.get('events',[]):
    if e['type']=='instantiate':
        for a in e['attributes']:
            if a['key']=='_contract_address': print(a['value'])
")
echo "    contract: $addr"
echo "$addr" > .contract-rebel2

echo "==> config"
terrad q wasm contract-state smart "$addr" '{"config":{}}'
