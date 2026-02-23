# ipc-exporter-rust

Prometheus metrics exporter for Bitcoin Core using IPC (Cap'n Proto over Unix socket). Connects directly to Bitcoin Core's multiprocess mode — no RPC or REST calls involved.

## How it works

The exporter subscribes to Bitcoin Core's `ChainNotifications` interface over IPC for real-time event counters (blocks connected/disconnected, mempool changes, flushes) and polls the `Chain` and `Node` interfaces every 60 seconds for gauges (height, mempool stats, peer count, bandwidth). Metrics are served in Prometheus text format over HTTP.

See [METRICS.md](METRICS.md) for the full metrics reference.

## Prerequisites

Bitcoin Core must be built with multiprocess support (`bitcoin-node` binary). The flake provides a pre-built package from [ryanofsky/bitcoin@pr/ipc](https://github.com/ryanofsky/bitcoin/tree/pr/ipc):

```
nix build .#bitcoin-node
```

## Building

Requires `capnproto` at build time for schema codegen.

With Nix (recommended):

```
nix build
```

Or inside a dev shell:

```
nix develop -c cargo build --release
```

## Usage

```
ipc-exporter-rust [--debug] [--metrics-addr HOST:PORT] <socket-path>
```

| Flag | Default | Description |
|------|---------|-------------|
| `--debug` | off | Verbose per-event logging to stderr |
| `--metrics-addr` | `127.0.0.1:9332` | Prometheus endpoint listen address |
| `<socket-path>` | required | Path to bitcoind's IPC Unix socket |

Example:

```
ipc-exporter-rust /var/lib/bitcoind/node.sock
```

Then scrape `http://127.0.0.1:9332/metrics` with Prometheus.

## NixOS module

The flake exports a NixOS module for running the exporter as a systemd service:

```nix
# flake.nix
{
  inputs.ipc-exporter.url = "github:bitcoin-dev-tools/ipc-exporter-rust";

  outputs = { nixpkgs, ipc-exporter, ... }: {
    nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
      modules = [
        ipc-exporter.nixosModules.default
        ({ ... }: {
          services.bitcoind-ipc-exporter = {
            enable = true;
            package = ipc-exporter.packages.x86_64-linux.default;
            socketPath = "/var/lib/bitcoind/node.sock";
            user = "bitcoind";
            group = "bitcoind";
            after = [ "bitcoind.service" ];
            bindsTo = [ "bitcoind.service" ];
          };
        })
      ];
    };
  };
}
```

See [CLAUDE.md](CLAUDE.md) for the full list of module options.

## License

MIT
