# List available commands
default:
    @just --list

# Install Cargo tools required for local development (nextest, llvm-cov)
install-tools:
    cargo install cargo-nextest --locked
    cargo install cargo-llvm-cov --locked
    rustup component add llvm-tools-preview

# Install all dependencies
install:
    pnpm install
    cargo fetch

# Build all projects
build:
    moon run :build
    cargo build --release

# Run Rust unit tests (via nextest)
test:
    cargo nextest run --workspace --exclude oxc-react-docgen-napi --locked

# Run TypeScript tests
test-ts:
    pnpm --filter @oxc-react-docgen/vite-plugin test

# Run all tests (Rust + TypeScript)
test-all: test test-ts

# Run benchmarks
bench:
    cargo bench --workspace --exclude oxc-react-docgen-napi

# Coverage report — opens HTML in browser
coverage:
    cargo llvm-cov nextest --workspace --exclude oxc-react-docgen-napi --locked --html --open

# Lint and format check
lint:
    cargo fmt --check
    cargo clippy --workspace --exclude oxc-react-docgen-napi --locked -- -D warnings

# Format code
fmt:
    cargo fmt
    pnpm exec prettier --write .

# Check licenses, advisories, and banned crates
deny:
    cargo deny check

# Check for typos
typos:
    typos

# Simulate full CI locally (lint → test → deny → typos → ts tests)
ci: lint test deny typos test-ts

# Run moon compare task (accuracy vs react-docgen + react-docgen-typescript)
compare:
    moon run validate:compare

# Clean build artifacts
clean:
    moon clean
    cargo clean
    rm -rf node_modules

# Setup toolchain via proto
setup:
    proto install
    pnpm install
    moon sync vcs-hooks

# Run moon commands
moon *args:
    moon {{args}}

# Run cargo commands
cargo *args:
    cargo {{args}}
