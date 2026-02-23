[private]
default:
    just --list

build:
    nix build

check:
    cargo clippy -- -D warnings
