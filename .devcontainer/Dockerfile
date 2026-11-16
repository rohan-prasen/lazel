FROM mcr.microsoft.com/devcontainers/rust:2-1-trixie

USER root
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 nftables \
    && rm -rf /var/lib/apt/lists/*

USER vscode
RUN rustup toolchain install stable --profile minimal \
        --component rustfmt --component clippy --component rust-src \
    && rustup default stable
