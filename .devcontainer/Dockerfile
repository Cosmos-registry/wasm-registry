FROM mcr.microsoft.com/devcontainers/rust:1-1-bullseye

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libssl-dev \
    jq \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add rustfmt clippy
