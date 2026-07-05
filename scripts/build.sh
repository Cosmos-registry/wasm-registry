#!/usr/bin/env bash
set -euo pipefail

cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo wasm

# docker run --rm -v "$(pwd)":/code --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry cosmwasm/optimizer:0.17.0
