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

run-bitcoind:
    #!/usr/bin/env bash
    if pgrep -x bitcoin > /dev/null; then
        echo "bitcoind is already running"
        exit 1
    fi
    /home/will/src/core/worktrees/pr-10102/build/bin/bitcoin -m node -chain=main -ipcbind=unix
