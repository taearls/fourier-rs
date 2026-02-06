# Development task runner for fourier-rs

# Run all checks: fmt, clippy, test
check: fmt-check lint test

# Run tests
test:
    cargo test --workspace

# Run clippy lints
lint:
    cargo clippy --workspace --tests -- -D warnings

# Format code
fmt:
    cargo fmt --all

# Check formatting (CI mode)
fmt-check:
    cargo fmt --all -- --check

# Run cargo-deny checks (licenses, bans, sources)
deny:
    cargo deny check licenses bans sources

# Build all crates
build:
    cargo build --workspace
