# Metrics Endpoint

The exporter serves Prometheus text format metrics over HTTP.

- **URL**: `http://127.0.0.1:9332/metrics`
- **Format**: [Prometheus text exposition format](https://prometheus.io/docs/instrumenting/exposition_formats/#text-based-format) (`text/plain; version=0.0.4`)
- **Method**: Any request to the TCP listener returns metrics (no path routing)

## Collection Methods

Metrics are collected via two mechanisms:

**Callback**: Updated in real-time via IPC subscription to Bitcoin Core's `ChainNotifications` interface. These reflect events as they happen with no delay.

**Polled**: Queried from Bitcoin Core's `Chain` and `Node` IPC interfaces every 60 seconds. An initial poll runs at startup so the endpoint returns real data immediately.

## Metrics Reference

### Callback Counters

These increment in real-time as Bitcoin Core pushes notifications over IPC.

| Metric | Type | Description |
|--------|------|-------------|
| `bitcoin_blocks_connected_total` | counter | Blocks connected to the chain |
| `bitcoin_blocks_disconnected_total` | counter | Blocks disconnected (reorgs) |
| `bitcoin_mempool_tx_added_total` | counter | Transactions added to the mempool |
| `bitcoin_mempool_tx_removed_total` | counter | Transactions removed from the mempool (expiry, sizelimit, reorg, block, conflict, replaced) |
| `bitcoin_tip_updates_total` | counter | Block tip update notifications |
| `bitcoin_chain_state_flushes_total` | counter | Chainstate flush events |

### Callback Gauges

| Metric | Type | Description |
|--------|------|-------------|
| `bitcoin_block_height` | gauge | Height of the most recently connected block (from `block_connected` callback) |

### Polled Gauges

Updated every 60 seconds via IPC queries to the `Chain` and `Node` interfaces.

| Metric | Type | Source | Description |
|--------|------|--------|-------------|
| `bitcoin_chain_height` | gauge | `Chain.getHeight()` | Current chain tip height |
| `bitcoin_ibd` | gauge | `Chain.isInitialBlockDownload()` | 1 if node is in initial block download, 0 otherwise |
| `bitcoin_verification_progress` | gauge | `Node.getVerificationProgress()` | Chain verification progress (0.0 to 1.0) |
| `bitcoin_mempool_size` | gauge | `Node.getMempoolSize()` | Number of transactions in the mempool |
| `bitcoin_mempool_bytes` | gauge | `Node.getMempoolDynamicUsage()` | Current mempool memory usage in bytes |
| `bitcoin_mempool_max_bytes` | gauge | `Node.getMempoolMaxUsage()` | Maximum mempool memory usage in bytes |
| `bitcoin_peers` | gauge | `Node.getNodeCount()` | Number of connected peers (inbound + outbound) |

### Polled Counters

| Metric | Type | Source | Description |
|--------|------|--------|-------------|
| `bitcoin_bytes_recv_total` | counter | `Node.getTotalBytesRecv()` | Total bytes received from peers |
| `bitcoin_bytes_sent_total` | counter | `Node.getTotalBytesSent()` | Total bytes sent to peers |

## Notes

- `bitcoin_block_height` (callback) and `bitcoin_chain_height` (polled) may briefly differ. The callback value updates instantly on block connection; the polled value updates on the 60s cycle.
- Polled counters (`bytes_recv`, `bytes_sent`) are monotonically increasing values from Bitcoin Core. They reset to 0 if bitcoind restarts.
- All IPC communication uses Cap'n Proto over a Unix socket. No RPC/REST calls are made to bitcoind.
