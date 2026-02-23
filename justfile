socket := "$HOME/.bitcoin/node.sock"

[private]
default:
    just --list

build:
    cargo build

check:
    cargo clippy -- -D warnings

run:
    nix run .# -- {{socket}}
