# List available commands
default:
    @just --list

# Install all dependencies
install:
    pnpm install
    cargo fetch

# Build all projects
build:
    moon run :build
    cargo build --release

# Run tests
test:
    moon run :test
    cargo test

# Lint and format check
lint:
    moon run :lint
    cargo clippy --all-targets --all-features
    cargo fmt --check

# Format code
fmt:
    cargo fmt
    pnpm exec prettier --write .

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
