bin := "niri-keymap-help"
dest := env_var("HOME") / ".local/bin"

# List recipes
default:
    @just --list

# Build an optimized release binary
build:
    cargo build --release --locked

# Format check, lint, and test
check:
    cargo fmt --all -- --check
    cargo clippy --all-targets --locked -- -D warnings
    cargo test --locked

# Build and install the binary into ~/.local/bin
deploy: build
    install -Dm755 target/release/{{bin}} {{dest}}/{{bin}}
    @echo "Installed {{dest}}/{{bin}}"
