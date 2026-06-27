#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Deploy CosmWasm contract to Gaia/wasmd-compatible chain.

Usage:
  scripts/deploy.sh [options]

Options:
  --binary <cmd>           CLI binary (default: gaiad)
  --chain-id <id>          Chain ID (required)
  --node <url>             RPC node URL (required)
  --from <key>             Local key name for signing (required)
  --wasm <path>            Wasm artifact path (default: artifacts/cosm_registry.wasm)
  --label <label>          Contract label (default: cosm-registry-<timestamp>)
  --init <json>            Instantiate JSON message (default: {})
  --admin <addr>           Admin address (default: address from --from key)
  --gas <value>            Gas setting (default: auto)
  --gas-adjustment <num>   Gas adjustment (default: 1.4)
  --gas-prices <coins>     Gas prices (default: 0.005uatom)
  --fees <coins>           Explicit fees (optional, overrides gas-prices behavior)
  --no-verify              Skip post-deploy smart queries
  -h, --help               Show this help

Examples:
  scripts/deploy.sh \
    --chain-id provider \
    --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443 \
    --from test-dev

  scripts/deploy.sh \
    --chain-id provider \
    --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443 \
    --from test-dev \
    --init '{"owner":"cosmos1...","treasury":"cosmos1..."}'
EOF
}

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

BINARY="gaiad"
CHAIN_ID=""
NODE=""
FROM_KEY=""
WASM_PATH="artifacts/cosm_registry.wasm"
LABEL="cosm-registry-$(date +%s)"
INIT_MSG='{}'
ADMIN_ADDR=""
GAS="auto"
GAS_ADJUSTMENT="1.4"
GAS_PRICES="0.005uatom"
FEES=""
VERIFY="true"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --binary)
      BINARY="$2"
      shift 2
      ;;
    --chain-id)
      CHAIN_ID="$2"
      shift 2
      ;;
    --node)
      NODE="$2"
      shift 2
      ;;
    --from)
      FROM_KEY="$2"
      shift 2
      ;;
    --wasm)
      WASM_PATH="$2"
      shift 2
      ;;
    --label)
      LABEL="$2"
      shift 2
      ;;
    --init)
      INIT_MSG="$2"
      shift 2
      ;;
    --admin)
      ADMIN_ADDR="$2"
      shift 2
      ;;
    --gas)
      GAS="$2"
      shift 2
      ;;
    --gas-adjustment)
      GAS_ADJUSTMENT="$2"
      shift 2
      ;;
    --gas-prices)
      GAS_PRICES="$2"
      shift 2
      ;;
    --fees)
      FEES="$2"
      shift 2
      ;;
    --no-verify)
      VERIFY="false"
      shift 1
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ -z "$CHAIN_ID" || -z "$NODE" || -z "$FROM_KEY" ]]; then
  echo "Missing required options: --chain-id, --node, --from" >&2
  usage
  exit 1
fi

require_cmd "$BINARY"
require_cmd jq

if [[ ! -f "$WASM_PATH" ]]; then
  echo "Wasm file not found: $WASM_PATH" >&2
  echo "Tip: build with CosmWasm optimizer to produce artifacts/cosm_registry.wasm" >&2
  exit 1
fi

if [[ -z "$ADMIN_ADDR" ]]; then
  ADMIN_ADDR="$($BINARY keys show "$FROM_KEY" -a)"
fi

echo "==> Storing wasm: $WASM_PATH"

TX_ARGS=(
  tx wasm store "$WASM_PATH"
  --from "$FROM_KEY"
  --chain-id "$CHAIN_ID"
  --node "$NODE"
  --gas "$GAS"
  --gas-adjustment "$GAS_ADJUSTMENT"
  -y
  --broadcast-mode block
  -o json
)

if [[ -n "$FEES" ]]; then
  TX_ARGS+=(--fees "$FEES")
else
  TX_ARGS+=(--gas-prices "$GAS_PRICES")
fi

STORE_JSON="$($BINARY "${TX_ARGS[@]}")"
TX_HASH="$(echo "$STORE_JSON" | jq -r '.txhash // .tx_response.txhash // empty')"

if [[ -z "$TX_HASH" ]]; then
  echo "Unable to read tx hash from store response" >&2
  echo "$STORE_JSON" >&2
  exit 1
fi

QUERY_TX_JSON="$($BINARY query tx "$TX_HASH" --node "$NODE" -o json)"
CODE_ID="$(echo "$QUERY_TX_JSON" | jq -r '[.. | objects | select(.type? == "store_code") | .attributes[]? | select(.key == "code_id") | .value][0] // empty')"

if [[ -z "$CODE_ID" ]]; then
  echo "Unable to extract code_id from tx: $TX_HASH" >&2
  echo "$QUERY_TX_JSON" >&2
  exit 1
fi

echo "==> Stored successfully"
echo "    tx_hash:  $TX_HASH"
echo "    code_id:  $CODE_ID"

echo "==> Instantiating code_id=$CODE_ID"

INST_ARGS=(
  tx wasm instantiate "$CODE_ID" "$INIT_MSG"
  --from "$FROM_KEY"
  --label "$LABEL"
  --admin "$ADMIN_ADDR"
  --chain-id "$CHAIN_ID"
  --node "$NODE"
  --gas "$GAS"
  --gas-adjustment "$GAS_ADJUSTMENT"
  -y
  --broadcast-mode block
  -o json
)

if [[ -n "$FEES" ]]; then
  INST_ARGS+=(--fees "$FEES")
else
  INST_ARGS+=(--gas-prices "$GAS_PRICES")
fi

INSTANTIATE_JSON="$($BINARY "${INST_ARGS[@]}")"
INST_TX_HASH="$(echo "$INSTANTIATE_JSON" | jq -r '.txhash // .tx_response.txhash // empty')"

if [[ -z "$INST_TX_HASH" ]]; then
  echo "Unable to read tx hash from instantiate response" >&2
  echo "$INSTANTIATE_JSON" >&2
  exit 1
fi

INST_QUERY_JSON="$($BINARY query tx "$INST_TX_HASH" --node "$NODE" -o json)"
CONTRACT_ADDR="$(echo "$INST_QUERY_JSON" | jq -r '[.. | objects | select(.type? == "instantiate") | .attributes[]? | select(.key == "_contract_address" or .key == "contract_address") | .value][0] // empty')"

if [[ -z "$CONTRACT_ADDR" ]]; then
  echo "Unable to extract contract address from tx: $INST_TX_HASH" >&2
  echo "$INST_QUERY_JSON" >&2
  exit 1
fi

echo "==> Instantiated successfully"
echo "    instantiate_tx: $INST_TX_HASH"
echo "    contract:       $CONTRACT_ADDR"

if [[ "$VERIFY" == "true" ]]; then
  echo "==> Verifying smart queries"
  OWNER_JSON="$($BINARY query wasm contract-state smart "$CONTRACT_ADDR" '{"get_owner":{}}' --node "$NODE" -o json)"
  PARAMS_JSON="$($BINARY query wasm contract-state smart "$CONTRACT_ADDR" '{"get_params":{}}' --node "$NODE" -o json)"
  echo "    get_owner: $OWNER_JSON"
  echo "    get_params: $PARAMS_JSON"
fi

echo "==> Done"
echo "CONTRACT_ADDR=$CONTRACT_ADDR"
echo "CODE_ID=$CODE_ID"
