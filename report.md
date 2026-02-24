# USDT Tracepoints vs IPC Interfaces: Comparison Report

This report compares two approaches for extracting runtime observability data from Bitcoin Core:

- **USDT tracepoints** — statically defined probes in the Bitcoin Core binary, consumed via eBPF (bpftrace, BCC). Available in standard builds compiled with `--enable-usdt` (or the equivalent CMake option).
- **IPC interfaces** — Cap'n Proto RPC over Unix socket, exposed by Bitcoin Core's multiprocess mode (`bitcoin-node`). Consumed by connecting to the socket and calling interface methods or subscribing to callbacks.

## Coverage Matrix

The following table maps every USDT tracepoint to its closest IPC equivalent, and lists IPC-only capabilities that have no tracepoint counterpart.

| USDT Tracepoint | IPC Equivalent | Delivery Match | Notes |
|---|---|---|---|
| `net:inbound_message` | None | — | No IPC equivalent. Raw P2P message content is USDT-only. |
| `net:outbound_message` | None | — | No IPC equivalent. |
| `net:inbound_connection` | `Node.handleNotifyNumConnectionsChanged` | Partial | IPC callback fires on count change, not per-connection. No address, network type, or connection type. |
| `net:outbound_connection` | `Node.handleNotifyNumConnectionsChanged` | Partial | Same limitation as above. |
| `net:closed_connection` | `Node.handleNotifyNumConnectionsChanged` | Partial | IPC only gives new total count. No peer identity or established timestamp. |
| `net:evicted_inbound_connection` | None | — | No IPC equivalent. |
| `net:misbehaving_connection` | None | — | No IPC equivalent. |
| `validation:block_connected` | `ChainNotifications.blockConnected` | Callback | Both are push-based. Different argument sets (see detailed comparison). |
| `utxocache:add` | `UtxoCacheTrace.add` | Callback | Identical fields. |
| `utxocache:spent` | `UtxoCacheTrace.spend` | Callback | Identical fields. |
| `utxocache:uncache` | `UtxoCacheTrace.uncache` | Callback | Identical fields. |
| `utxocache:flush` | `ChainNotifications.chainStateFlushed` | Partial | IPC callback only signals the event with a chainstate role and locator. No flush duration, mode, cache size, memory usage, or prune flag. |
| `mempool:added` | `ChainNotifications.transactionAddedToMempool` | Callback | IPC provides raw serialized tx bytes. USDT provides pre-parsed txid, vsize, and fee. |
| `mempool:removed` | `ChainNotifications.transactionRemovedFromMempool` | Callback | IPC provides raw tx bytes + reason enum. USDT provides txid, reason string, vsize, fee, and entry time. |
| `mempool:replaced` | None | — | No IPC equivalent. |
| `mempool:rejected` | None | — | No IPC equivalent. |
| `coin_selection:selected_coins` | None | — | Wallet-specific. No IPC tracing interface for coin selection. |
| `coin_selection:normal_create_tx_internal` | None | — | Wallet-specific. |
| `coin_selection:attempting_aps_create_tx` | None | — | Wallet-specific. |
| `coin_selection:aps_create_tx_internal` | None | — | Wallet-specific. |

### IPC-Only Capabilities (No USDT Equivalent)

These are available through IPC polling or callbacks but have no corresponding USDT tracepoint.

| IPC Method | Interface | Delivery | Description |
|---|---|---|---|
| `getNodeCount(flags)` | Node | Poll | Total peer count by connection direction. |
| `getNodesStats()` | Node | Poll | Per-peer detailed stats (address, version, subver, send/recv bytes, ping times, network, connection type, BIP152 status, etc.). |
| `getTotalBytesRecv()` | Node | Poll | Cumulative bytes received across all peers. |
| `getTotalBytesSent()` | Node | Poll | Cumulative bytes sent across all peers. |
| `getMempoolSize()` | Node | Poll | Number of transactions in mempool. |
| `getMempoolDynamicUsage()` | Node | Poll | Mempool memory usage in bytes. |
| `getMempoolMaxUsage()` | Node | Poll | Maximum allowed mempool memory. |
| `getHeaderTip()` | Node | Poll | Header chain height and block time. |
| `getVerificationProgress()` | Node | Poll | Chain verification progress (0.0 to 1.0). |
| `isInitialBlockDownload()` | Node | Poll | Whether the node is in IBD. |
| `estimateSmartFee()` | Chain | Poll | Fee estimation with confirmation target. |
| `mempoolMinFee()` | Chain | Poll | Minimum fee to enter mempool. |
| `relayMinFee()` | Chain | Poll | Minimum relay fee. |
| `havePruned()` / `getPruneHeight()` | Chain | Poll | Pruning status and height. |
| `getBanned()` | Node | Poll | Ban list. |
| `getNetworkActive()` | Node | Poll | Whether networking is enabled. |
| `handleNotifyBlockTip` | Node | Callback | Block tip notification with sync state and verification progress. |
| `handleNotifyHeaderTip` | Node | Callback | Header tip notification with sync state and verification progress. |
| `handleNotifyNetworkActiveChanged` | Node | Callback | Network active/inactive toggle events. |
| `handleBannedListChanged` | Node | Callback | Ban list change events. |

## Detailed Comparisons by Category

### Networking

**USDT** provides 7 tracepoints covering the full lifecycle of P2P connections and messages:

- Per-message visibility (`inbound_message`, `outbound_message`) with peer ID, address, connection type, message type, size, and raw message bytes.
- Per-connection events (`inbound_connection`, `outbound_connection`, `closed_connection`, `evicted_inbound_connection`) with peer ID, address, connection type, network enum (IPv4/IPv6/Tor/I2P/CJDNS), and connection counts or timestamps.
- Misbehavior detection (`misbehaving_connection`) with peer ID and reason string.

**IPC** has no per-message or per-connection-event visibility. The closest equivalents are:

- `Node.handleNotifyNumConnectionsChanged` — callback that fires when the total connection count changes, providing only the new count (int32). No peer identity, address, network type, or direction.
- `Node.getNodesStats()` — polling method returning a detailed `NodeStats` struct per connected peer. Fields include peer ID, address, connection type, network, protocol version, subversion, send/recv bytes (total and per message type), ping times, BIP152 high-bandwidth status, and more.

The key gap: USDT can trace every individual P2P message and connection event in real-time. IPC can snapshot the current peer set via polling, but cannot observe individual messages, connection establishment/teardown events, evictions, or misbehavior. For a metrics exporter, USDT enables message-rate counters, per-message-type histograms, and connection churn tracking. IPC only enables periodic snapshots of aggregate peer state.

However, IPC's `getNodesStats()` provides per-peer detail (subversion, ping latency, BIP152 status, per-message-type byte counters) that USDT tracepoints don't expose in aggregate form.

### Block Validation

Both provide a push-based notification when a block is connected. The arguments differ significantly:

| Field | USDT `validation:block_connected` | IPC `ChainNotifications.blockConnected` |
|---|---|---|
| Block hash | 32-byte hash (little-endian) | `BlockInfo.hash` (Data) |
| Height | int32 | `BlockInfo.height` (Int32) |
| Previous block hash | — | `BlockInfo.prevHash` (Data) |
| Transaction count | uint64 | — |
| Input count | int32 | — |
| SigOps count | uint64 | — |
| Connection duration (ns) | uint64 | — |
| Chain time max | — | `BlockInfo.chainTimeMax` (UInt32) |
| File number / data position | — | `BlockInfo.fileNumber`, `BlockInfo.dataPos` |
| Raw block data | — | `BlockInfo.data` (Data, optional) |
| Undo data | — | `BlockInfo.undoData` (Data, optional) |
| Chainstate role | — | `ChainstateRole` (validated/historical) |

USDT is oriented toward performance analysis: it provides tx count, input count, sigops, and the nanosecond duration of `ConnectBlock`. IPC is oriented toward chain tracking: it provides the block data itself, previous hash for chain linking, chainstate role (distinguishing assumed-valid background validation from normal validation), and on-disk location.

Neither is a superset of the other. An exporter wanting block connection latency metrics must use USDT. An exporter wanting to distinguish historical vs validated chainstate roles must use IPC.

IPC also provides `ChainNotifications.blockDisconnected` and `ChainNotifications.updatedBlockTip` callbacks, which have no direct USDT equivalents (there is no `validation:block_disconnected` or `validation:updated_block_tip` tracepoint).

### UTXO Cache

USDT and IPC are nearly identical for `add`, `spent`, and `uncache` events:

| Field | USDT `utxocache:add/spent/uncache` | IPC `UtxoCacheTrace.add/spend/uncache` |
|---|---|---|
| Transaction hash | 32-byte pointer (little-endian) | `UtxoInfo.outpointHash` (Data) |
| Output index | uint32 | `UtxoInfo.outpointN` (UInt32) |
| Height | uint32 | `UtxoInfo.height` (UInt32) |
| Value (satoshis) | int64 | `UtxoInfo.value` (Int64) |
| Is coinbase | bool | `UtxoInfo.isCoinbase` (Bool) |

These are functionally equivalent. Both fire on every individual UTXO cache operation. Both include the same five fields with the same semantics.

For **flush events**, the picture differs substantially:

| Field | USDT `utxocache:flush` | IPC `ChainNotifications.chainStateFlushed` |
|---|---|---|
| Flush duration | int64 (microseconds) | — |
| Flush mode | uint32 (NONE/IF_NEEDED/PERIODIC/FORCE_FLUSH/FORCE_SYNC) | — |
| Coins count (before flush) | uint64 | — |
| Memory usage (before flush) | uint64 bytes | — |
| Triggered by pruning | bool | — |
| Chainstate role | — | `ChainstateRole` (validated/historical) |
| Locator | — | Data (block locator) |

USDT's `utxocache:flush` is a performance-oriented probe providing timing, cache size, memory, and flush mode. IPC's `chainStateFlushed` is a state-tracking signal with the chainstate role and a block locator, but no performance metrics. An exporter wanting flush duration or cache size metrics must use USDT.

### Mempool

Both approaches provide notifications for transactions entering and leaving the mempool, but the data differs:

**Transaction added:**

| Field | USDT `mempool:added` | IPC `transactionAddedToMempool` |
|---|---|---|
| Transaction ID | 32-byte hash | — (must deserialize from tx bytes) |
| Virtual size (vbytes) | int32 | — (must deserialize from tx bytes) |
| Fee (satoshis) | int64 | — (not available without additional lookups) |
| Raw transaction | — | Full serialized transaction (Data) |

IPC provides the complete serialized transaction, from which the txid and vsize can be derived by deserialization. However, the fee is not directly available since it requires knowing the input values (which the serialized transaction alone doesn't contain).

**Transaction removed:**

| Field | USDT `mempool:removed` | IPC `transactionRemovedFromMempool` |
|---|---|---|
| Transaction ID | 32-byte hash | — (must deserialize from tx bytes) |
| Removal reason | C-string (max 9 chars) | Int32 enum (0=expiry, 1=sizelimit, 2=reorg, 3=block, 4=conflict, 5=replaced) |
| Virtual size | int32 | — (must deserialize) |
| Fee (satoshis) | int64 | — (not available) |
| Entry time | uint64 (epoch seconds) | — |
| Raw transaction | — | Full serialized transaction (Data) |

USDT provides pre-extracted metrics (vsize, fee, entry time) useful for direct counter/gauge updates. IPC provides the raw transaction and a reason enum, requiring deserialization work and lacking fee/entry-time data.

**USDT-only mempool events:**

- `mempool:replaced` — fires when a transaction is replaced via RBF, providing both the replaced and replacement transaction details (txid, vsize, fee, entry time) plus a flag indicating whether the replacement is a single transaction or a package. No IPC equivalent.
- `mempool:rejected` — fires when a transaction fails mempool admission, providing the txid and rejection reason string (up to 118 chars). No IPC equivalent.

### Coin Selection (USDT-Only)

Four tracepoints in the `coin_selection` context track wallet coin selection behavior:

- `selected_coins` — algorithm name, target value, waste metric, selected value
- `normal_create_tx_internal` — success/failure, fee, change position
- `attempting_aps_create_tx` — Avoid Partial Spends attempt
- `aps_create_tx_internal` — APS result, fee, change position

These are wallet-specific and have no IPC equivalent. The IPC `Wallet` interface exposes transactional wallet operations (create, sign, send) but no internal algorithm tracing. Note that the multiprocess `bitcoin-node` binary does not include wallet functionality by default; wallet runs as a separate `bitcoin-wallet` process.

## Delivery Model Comparison

| Aspect | USDT | IPC |
|---|---|---|
| Push events | All 20 tracepoints fire synchronously at the instrumentation site | 9 callbacks (6 chain notifications + 3 UTXO cache trace) |
| Polling | Not applicable (all push) | Required for state snapshots (peer count, mempool stats, bandwidth, IBD status, verification progress, fee estimates) |
| Latency | Nanoseconds (in-process probe) | Microseconds to low milliseconds (Unix socket RPC round-trip for polls; callbacks are pushed over the socket) |
| Overhead when idle | Zero (semaphore-guarded; probes are NOPs when no tracer is attached) | Minimal for callbacks (socket idle). Polling has periodic cost. |
| Overhead when active | eBPF program execution per probe hit; stack size limited to 512 bytes | Cap'n Proto serialization/deserialization per event; no stack limit |

## Overhead on Bitcoin Core

A critical difference between the two approaches is the cost imposed on the Bitcoin Core process itself when emitting events.

### USDT

The `TRACEPOINT` macro in `src/util/trace.h` is guarded by a semaphore:

```c
#define TRACEPOINT(context, event, ...)        \
    do {                                        \
        if (TRACEPOINT_ACTIVE(context, event))  \
            STAP_PROBEV(context, event, ...);   \
    } while(0)
```

**When no tracer is attached:** the semaphore is 0, the branch is not taken, and the probe site is effectively a NOP. The cost is a single conditional branch — essentially zero.

**When a tracer is attached:** the semaphore is incremented by the tracer at attach time. The probe fires, transferring control to the kernel where the eBPF program executes. Typical per-event cost is 1-10 microseconds depending on eBPF program complexity. Crucially, the eBPF program runs asynchronously in kernel context — the Bitcoin Core thread is **not blocked** waiting for the consumer to process the event.

### IPC Callbacks

IPC notifications use Cap'n Proto RPC. When Bitcoin Core fires a notification (e.g., `blockConnected`), the dispatch path through `libmultiprocess` works as follows:

1. The calling thread (e.g., the validation thread) invokes `clientInvoke()` which schedules the Cap'n Proto request on the event loop thread.
2. The event loop thread serializes the message and writes it to the Unix socket.
3. **The calling thread blocks** (`waiter->wait()`) until the remote subscriber sends a response.

There is no async buffering or fire-and-forget path. Every notification is a **synchronous RPC round-trip** from the perspective of the calling thread.

The blocking cost depends on two factors: socket round-trip latency and subscriber processing time. Even with an empty subscriber callback, the minimum overhead is the serialization + socket write + socket read + deserialization round-trip, typically 10-100 microseconds per event.

#### ChainNotifications vs UtxoCacheTrace

The two callback interfaces differ in an important way:

- **ChainNotifications** methods (`blockConnected`, `transactionAddedToMempool`, etc.) include a `context: Proxy.Context` parameter in the Cap'n Proto schema. This causes the subscriber-side callback to run in a **dedicated worker thread**, so the IPC event loop remains free to handle other traffic while the callback executes.

- **UtxoCacheTrace** methods (`add`, `spend`, `uncache`) have **no `context` parameter**. This means the subscriber callback runs **synchronously on the event loop thread itself**, blocking all other IPC traffic for the duration of the callback.

#### Impact on Block Validation

During block connection, UTXO cache operations fire at extremely high frequency — roughly one `add` per output and one `spend` per input. A typical block with 2,000 transactions generates ~10,000 UTXO cache events.

With IPC subscribers attached:
- The validation thread blocks for each event's round-trip, paying ~10-100 microseconds per UTXO operation.
- 10,000 events × 10 μs = **~100 ms of validation thread stall per block** (optimistic estimate).
- Since UtxoCacheTrace callbacks lack a `context` parameter, they also serialize on the event loop thread, creating a bottleneck for any concurrent IPC traffic.

With USDT (tracer attached):
- Each probe hit costs ~1-10 μs of eBPF execution in kernel context.
- The validation thread is not blocked — eBPF runs asynchronously.
- 10,000 events × ~1 μs = **~10 ms** of total eBPF overhead, none of it blocking validation.

With USDT (no tracer):
- **Zero overhead.** The semaphore check evaluates to false and the probe is skipped entirely.

#### Locking

The UTXO cache dispatch path in `coins.cpp` holds a mutex while iterating over trace subscribers:

```cpp
if (m_traces) {
    LOCK(m_traces->mutex);
    for (const auto& trace : m_traces->utxo_cache) {
        trace->add(interfaces::UtxoInfo{...});
    }
}
```

The lock is held for the **entire duration of all IPC round-trips** to all subscribers. This means subscriber latency directly extends the lock hold time, blocking any concurrent trace registration/deregistration.

### Summary

| Scenario | Per-Event Cost | Calling Thread Blocks | Event Loop Blocks |
|---|---|---|---|
| USDT (no tracer) | ~0 (NOP) | No | N/A |
| USDT (tracer attached) | 1-10 μs | No | N/A |
| IPC ChainNotifications | 10-100+ μs | Yes (full round-trip) | No (has `context`) |
| IPC UtxoCacheTrace | 10-100+ μs | Yes (full round-trip) | Yes (no `context`) |

For a metrics exporter that subscribes to UtxoCacheTrace, the overhead during IBD or reindex — where blocks are connected continuously — is substantial. USDT imposes no cost when no consumer is attached and minimal, non-blocking cost when one is.

## Trade-offs

| Consideration | USDT | IPC |
|---|---|---|
| **Bitcoin Core build** | Standard build with tracepoint support (`-DWITH_USDT=ON` or `--enable-usdt`) | Multiprocess build required (`bitcoin-node` binary) |
| **Runtime requirements** | Linux kernel >= 4.18 with eBPF support; root or `CAP_BPF` + `CAP_PERFMON` | Unix socket access; no special kernel features |
| **Consumer language** | C (BCC/libbpf), bpftrace scripts, or any language with BPF bindings | Any language with Cap'n Proto support (Rust, C++, Python, Go, etc.) |
| **Consumer complexity** | Must write eBPF programs; limited stack (512 bytes); pointer reading via `bpf_probe_read_user`; arg count limited to 6 on x86_64 with bpftrace | Standard RPC client code; structured messages; no kernel interaction |
| **Binary data** | Raw bytes via pointer; must manually parse hashes, transactions | Structured Cap'n Proto messages with typed fields |
| **Per-event data richness** | Pre-extracted scalar fields (vsize, fee, duration, etc.) | Structured objects (raw tx bytes, BlockInfo, UtxoInfo) requiring deserialization for some fields |
| **Networking visibility** | Full per-message and per-connection event tracing with raw bytes | Aggregate stats via polling only; no individual message or connection events |
| **UTXO cache visibility** | Full (add/spend/uncache/flush with performance metrics) | add/spend/uncache identical; flush lacks performance metrics |
| **Mempool event coverage** | 4 events: added, removed, replaced, rejected | 2 events: added, removed. No replaced/rejected. |
| **Wallet/coin selection** | 4 tracepoints for algorithm tracing | Not available (wallet is a separate process in multiprocess mode) |
| **State queries** | Not available (tracepoints are event-only) | Extensive: fee estimation, mempool stats, peer details, block lookups, IBD status, etc. |
| **Stability** | Tracepoints are considered a stable interface in Bitcoin Core | IPC schemas are tied to the multiprocess branch; interface may evolve |
| **Multi-consumer** | Multiple eBPF programs can attach to the same tracepoint simultaneously | Single socket connection per exporter instance (though Core can accept multiple) |
| **Platform support** | Linux only | Any platform supporting Unix sockets |

## Summary

USDT tracepoints and IPC interfaces are complementary rather than competing. USDT excels at high-frequency, low-overhead event tracing with pre-extracted performance metrics — particularly for networking (the entire `net:*` category is USDT-only) and performance analysis (block connection duration, flush timing). IPC excels at structured state queries and provides a more accessible programming model, but has narrower event coverage and lacks the raw-message and performance-timing data that USDT provides.

A comprehensive monitoring solution could use both: IPC for state polling and the events it does cover (chain notifications, UTXO cache), and USDT for networking visibility, mempool replaced/rejected events, flush performance, block connection timing, and coin selection tracing.
