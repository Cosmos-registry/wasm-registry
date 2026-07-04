# Wasm Registry - CosmWasm Smart Contract Workspace

This repository contains the Wasm Registry smart contract, built with Rust and CosmWasm.
The contract provides an on-chain registry for Cosmos chain metadata and public infrastructure endpoints (RPC, REST, gRPC, WSS), with an economic model to manage endpoint quality over time.

## Project Purpose

The goal of Wasm Registry is to provide a canonical, queryable source of chain and endpoint information for Cosmos ecosystem tools.

It supports:

- Registering and updating chain metadata.
- Registering infrastructure endpoints with minimum collateral.
- Managing endpoint lifecycle through lazy rent accounting.
- Exporting a chain-registry-like API projection.

## Prerequisites

- Docker running locally.

## Useful Commands

- Format check: `cargo fmt --all --check`
- Lint: `cargo clippy --all-targets -- -D warnings`
- Unit and integration tests: `cargo test`
- Build optimized wasm: `cargo wasm`
- Generate JSON schemas: `cargo schema`

## Repository Structure

- `src/`: contract implementation (entrypoints, messages, state, errors).
- `examples/schema.rs`: schema generation binary.
- `schema/`: generated JSON schemas.
- `tests/`: integration tests with cw-multi-test.
- `scripts/`: helper scripts for build, deploy, and schema generation.
- `.devcontainer/`: reproducible development environment.

## Implemented V1 Contract Scope

### Execute Messages

- `RegisterChain`: creates a chain with normalized metadata and assets.
- `UpdateChainMeta`: updates metadata for an existing chain (chain owner or admin).
- `RegisterEndpoint`: adds an RPC/REST/gRPC/WSS endpoint with required minimum deposit.
- `TopUpEndpoint`: adds funds to an endpoint deposit (endpoint owner or admin).
- `RemoveEndpoint`: removes an endpoint and refunds remaining deposit (endpoint owner or admin).
- `SetParams`: updates economic and protocol parameters (admin only).
- `SetEndpointFlags`: updates `verified` and `preferred` flags (admin only).

### Query Messages

- `GetChain`: returns chain metadata and active endpoints.
- `GetChains`: paginated list of chains.
- `GetEndpoints`: endpoint list filtered by type and active state.
- `ExportChainJson`: chain-registry-compatible projection for `apis.rpc/rest/grpc/wss`.
- `GetOwner`: returns contract owner and treasury.
- `GetParams`: returns active registry parameters.

## Security And Validation Model

- Strict authorization controls:
  - admin only for `SetParams` and `SetEndpointFlags`.
  - chain owner or admin for `UpdateChainMeta`.
  - endpoint owner or admin for `TopUpEndpoint` and `RemoveEndpoint`.
- Input validation:
  - `chain_id` constrained to lowercase alphanumeric and hyphen rules.
  - required text fields validated for emptiness and max length.
  - endpoint URL scheme validated by endpoint kind.
  - URL normalization and uniqueness index to block duplicate variants.
- Economic protections:
  - minimum deposit required for endpoint registration.
  - lazy rent charging per epoch.
  - endpoint automatically becomes inactive when deposit is exhausted.
  - rent accumulation routed to logical treasury accounting.

## Test Coverage (Integration)

Current integration tests cover:

- Chain registration and `chain.json` export compatibility.
- Duplicate endpoint rejection after URL normalization.
- Rejection when deposit is below minimum.
- Admin-only parameter update enforcement.
- Lazy expiry and endpoint reactivation after top-up.

## Build Artifacts

After a successful wasm build, compiled artifacts are located in `artifacts/`.

## Deployment Example

```bash
~/go/bin/gaiad tx wasm store artifacts/cosm_registry.wasm \
  --from test-dev \
  --chain-id provider \
  --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443 \
  --gas auto --gas-adjustment 1.4 --gas-prices 0.005uatom

~/go/bin/gaiad query wasm list-code \
  --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443 \
  --output json --reverse | jq -r '.code_infos[-1].code_id'

export CODE_ID=<code_id>

INIT='{"owner":"cosmos1abcd","treasury":"cosmos1abcd"}'

~/go/bin/gaiad tx wasm instantiate ${CODE_ID} "$INIT" \
  --label "chain-registry" \
  --admin cosmos1abcd \
  --from test-dev \
  --chain-id provider \
  --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443 \
  --gas auto --gas-adjustment 1.4 --gas-prices 0.005uatom
```

## Upgrade And Migration Example

```bash
export ACCOUNT=test-dev
export ADDRESS=$(~/go/bin/gaiad keys show ${ACCOUNT} --address)

~/go/bin/gaiad query wasm list-contracts-by-creator ${ADDRESS} \
  --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443 \
  -o json

~/go/bin/gaiad tx wasm store artifacts/cosm_registry.wasm \
  --from ${ACCOUNT} \
  --chain-id provider \
  --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443 \
  --gas auto --gas-adjustment 1.4 --gas-prices 0.005uatom \
  -y --broadcast-mode sync -o json

export NEW_CODE_ID=<code_id>
export CONTRACT_ADDR=<contract_address>

~/go/bin/gaiad tx wasm migrate ${CONTRACT_ADDR} ${NEW_CODE_ID} '{}' \
  --from ${ACCOUNT} \
  --chain-id provider \
  --node https://rpc.provider-sentry-01.hub-testnet.polypore.xyz:443 \
  --gas auto --gas-adjustment 1.4 --gas-prices 0.005uatom \
  -y --broadcast-mode sync -o json
```

## Notes

- Replace placeholder addresses and IDs with your environment values.
- Always review generated schemas and run tests before deployment.
