# ipc-exporter-rust

Prometheus metrics exporter for Bitcoin Core using IPC (Cap'n Proto over Unix socket). Replaces the older USDT/BCC + RPC polling approach with direct subscription-based metrics from Bitcoin Core's multiprocess mode.

## Prerequisites

Bitcoin Core must be built with multiprocess support (the `bitcoin-node` binary from the pr-10102 branch or equivalent). The exporter connects to its IPC Unix socket.

## Architecture

Single binary that does three things concurrently:

1. **IPC subscription** — subscribes to `ChainNotifications` via Cap'n Proto RPC. Callbacks fire in real-time for block/mempool/flush events and update atomic counters.
2. **Polling loop** — queries `Chain` and `Node` interfaces every 60 seconds for gauges (height, mempool stats, peer count, bandwidth). Initial poll runs at startup.
3. **HTTP server** — raw `TcpListener` on `127.0.0.1:9332` (configurable) serving Prometheus text format. No HTTP framework — just reads one request and writes the response.

### Source Layout

| File | Responsibility |
|------|----------------|
| `src/main.rs` | CLI parsing, signal handling, poll loop, orchestration. Declares capnp generated modules. |
| `src/rpc.rs` | `RpcInterface` — IPC handshake and all Cap'n Proto RPC query methods. |
| `src/metrics.rs` | `Metrics` struct (atomic counters/gauges) and `format_metrics()` Prometheus formatter. |
| `src/server.rs` | `serve_metrics()` — TCP listener serving the Prometheus text endpoint. |
| `src/notifications.rs` | `NotificationHandler` — `ChainNotifications` callback impl that updates metrics. |

All metrics live in a `Metrics` struct using stdlib atomics (`AtomicU64`, `AtomicI32`, `AtomicBool`, etc.), shared via `Arc`. The `f64` verification_progress is stored as bits in an `AtomicU64`.

The IPC handshake sequence: `Init.construct()` → `ThreadMap.makeThread()` → `Init.makeChain()` / `Init.makeNode()`. Every RPC call requires passing a `thread` context.

## Cap'n Proto Schemas

Located in `schema/`, sourced from Bitcoin Core's multiprocess branch. `build.rs` compiles them into Rust code at build time via `capnpc`. The generated modules are included with `include!(concat!(env!("OUT_DIR"), "/<name>_capnp.rs"))`.

Schemas: chain, common, echo, handler, init, mining, node, wallet, mp/proxy. Not all are actively used — chain, node, handler, init, and proxy are the ones the exporter calls.

## CLI

```
ipc-exporter-rust [--debug] [--metrics-addr HOST:PORT] <socket-path>
```

- `--debug` — verbose per-event and per-poll logging to stderr (off by default)
- `--metrics-addr` — listen address for Prometheus endpoint (default `127.0.0.1:9332`)
- `<socket-path>` — path to bitcoind's IPC Unix socket

Without `--debug`, only startup/shutdown messages are printed to stderr.

## Metrics

See `METRICS.md` for the full reference. Summary:

- 6 callback counters (blocks connected/disconnected, mempool tx add/remove, tip updates, flushes)
- 1 callback gauge (block_height from block_connected)
- 7 polled gauges (chain_height, ibd, verification_progress, mempool_size/bytes/max, peers)
- 2 polled counters (bytes_recv, bytes_sent)

All metric names are prefixed `bitcoin_`.

## Building

Requires `capnproto` (the compiler, not the Rust crate) at build time for schema codegen.

```
nix develop -c cargo build
```

Or via Nix:

```
nix build
```

## NixOS Module

The flake exports `nixosModules.default` (defined in `module.nix`). It creates a systemd service `bitcoind-ipc-exporter`.

### Module Options (`services.bitcoind-ipc-exporter`)

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enable` | bool | `false` | Enable the service |
| `package` | package | — (required) | The ipc-exporter-rust package |
| `socketPath` | string | — (required) | Path to bitcoind IPC Unix socket |
| `metricsAddr` | string | `"127.0.0.1:9332"` | Prometheus endpoint listen address |
| `debug` | bool | `false` | Verbose stderr logging |
| `user` | string | `"root"` | Systemd service user |
| `group` | string | `"root"` | Systemd service group |
| `after` | list of strings | `[]` | Systemd `After=` dependencies |
| `bindsTo` | list of strings | `[]` | Systemd `BindsTo=` dependencies |

### Integrating into another flake

```nix
# flake.nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    ipc-exporter.url = "github:bitcoin-dev-tools/ipc-exporter-rust";
  };

  outputs = { nixpkgs, ipc-exporter, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ipc-exporter.nixosModules.default
        ./configuration.nix
      ];
      specialArgs = { inherit ipc-exporter; };
    };
  };
}
```

```nix
# configuration.nix
{ ipc-exporter, ... }:
{
  services.bitcoind-ipc-exporter = {
    enable = true;
    package = ipc-exporter.packages.x86_64-linux.default;
    socketPath = "/var/lib/bitcoind-mainnet/node.sock";
    user = "bitcoind-mainnet";
    group = "bitcoind-mainnet";
    after = [ "bitcoind-mainnet.service" ];
    bindsTo = [ "bitcoind-mainnet.service" ];
  };

  # Add a Prometheus scrape target
  services.prometheus.scrapeConfigs = [{
    job_name = "bitcoind-ipc";
    static_configs = [{ targets = [ "127.0.0.1:9332" ]; }];
  }];
}
```

## Not Yet Implemented

- Reconnect on bitcoind restart (currently the exporter exits; systemd `Restart=on-failure` handles it)
- Grafana dashboard JSON provisioning
- Dashboard parity validation against the existing USDT/RPC exporter

## Development Rules

- Keep this CLAUDE.md updated when making structural changes (new modules, changed architecture, new CLI flags, etc.).
