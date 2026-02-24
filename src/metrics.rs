use std::fmt::Write;
// Relaxed ordering is sufficient: each field is an independent counter/gauge with no
// cross-field invariants, so we only need atomicity, not inter-thread ordering guarantees.
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

pub struct Metrics {
    pub blocks_connected: AtomicU64,
    pub blocks_disconnected: AtomicU64,
    pub mempool_tx_added: AtomicU64,
    pub mempool_tx_removed: AtomicU64,
    pub tip_updates: AtomicU64,
    pub chain_state_flushes: AtomicU64,
    pub block_height: AtomicI32,
    pub header_height: AtomicI32,
    pub ibd: AtomicBool,
    pub verification_progress: AtomicU64,
    pub mempool_size: AtomicU64,
    pub mempool_bytes: AtomicU64,
    pub mempool_max: AtomicU64,
    pub peers: AtomicU64,
    pub bytes_recv: AtomicI64,
    pub bytes_sent: AtomicI64,
    pub utxo_cache_add: AtomicU64,
    pub utxo_cache_spend: AtomicU64,
    pub utxo_cache_uncache: AtomicU64,
    pub utxo_cache_add_value: AtomicU64,
    pub utxo_cache_spend_value: AtomicU64,
    pub utxo_cache_uncache_value: AtomicU64,
}

impl Metrics {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            blocks_connected: AtomicU64::new(0),
            blocks_disconnected: AtomicU64::new(0),
            mempool_tx_added: AtomicU64::new(0),
            mempool_tx_removed: AtomicU64::new(0),
            tip_updates: AtomicU64::new(0),
            chain_state_flushes: AtomicU64::new(0),
            block_height: AtomicI32::new(-1),
            header_height: AtomicI32::new(-1),
            ibd: AtomicBool::new(false),
            verification_progress: AtomicU64::new(0),
            mempool_size: AtomicU64::new(0),
            mempool_bytes: AtomicU64::new(0),
            mempool_max: AtomicU64::new(0),
            peers: AtomicU64::new(0),
            bytes_recv: AtomicI64::new(0),
            bytes_sent: AtomicI64::new(0),
            utxo_cache_add: AtomicU64::new(0),
            utxo_cache_spend: AtomicU64::new(0),
            utxo_cache_uncache: AtomicU64::new(0),
            utxo_cache_add_value: AtomicU64::new(0),
            utxo_cache_spend_value: AtomicU64::new(0),
            utxo_cache_uncache_value: AtomicU64::new(0),
        })
    }
}

pub fn format_metrics(m: &Metrics) -> String {
    let mut s = String::with_capacity(2048);
    macro_rules! counter {
        ($name:expr, $help:expr, $val:expr) => {
            let _ = writeln!(s, "# HELP {0} {1}\n# TYPE {0} counter\n{0} {2}", $name, $help, $val);
        };
    }
    macro_rules! gauge {
        ($name:expr, $help:expr, $val:expr) => {
            let _ = writeln!(s, "# HELP {0} {1}\n# TYPE {0} gauge\n{0} {2}", $name, $help, $val);
        };
    }
    counter!("bitcoin_blocks_connected_total", "Blocks connected via ChainNotifications.", m.blocks_connected.load(Relaxed));
    counter!("bitcoin_blocks_disconnected_total", "Blocks disconnected via ChainNotifications.", m.blocks_disconnected.load(Relaxed));
    counter!("bitcoin_mempool_tx_added_total", "Transactions added to mempool.", m.mempool_tx_added.load(Relaxed));
    counter!("bitcoin_mempool_tx_removed_total", "Transactions removed from mempool.", m.mempool_tx_removed.load(Relaxed));
    counter!("bitcoin_tip_updates_total", "Block tip updates received.", m.tip_updates.load(Relaxed));
    counter!("bitcoin_chain_state_flushes_total", "Chain state flush events.", m.chain_state_flushes.load(Relaxed));
    gauge!("bitcoin_block_height", "Height of the last connected block.", m.block_height.load(Relaxed));
    gauge!("bitcoin_header_height", "Height of the best known header.", m.header_height.load(Relaxed));
    gauge!("bitcoin_ibd", "Whether the node is in initial block download (1=yes, 0=no).", m.ibd.load(Relaxed) as u8);
    gauge!("bitcoin_verification_progress", "Estimated chain verification progress (0.0 to 1.0).", f64::from_bits(m.verification_progress.load(Relaxed)));
    gauge!("bitcoin_mempool_size", "Number of transactions in the mempool.", m.mempool_size.load(Relaxed));
    gauge!("bitcoin_mempool_bytes", "Memory usage of the mempool in bytes.", m.mempool_bytes.load(Relaxed));
    gauge!("bitcoin_mempool_max_bytes", "Maximum mempool size in bytes.", m.mempool_max.load(Relaxed));
    gauge!("bitcoin_peers", "Number of connected peers.", m.peers.load(Relaxed));
    counter!("bitcoin_bytes_recv_total", "Total bytes received from peers.", m.bytes_recv.load(Relaxed));
    counter!("bitcoin_bytes_sent_total", "Total bytes sent to peers.", m.bytes_sent.load(Relaxed));
    counter!("bitcoin_utxo_cache_add_total", "UTXOs added to the cache.", m.utxo_cache_add.load(Relaxed));
    counter!("bitcoin_utxo_cache_spend_total", "UTXOs spent from the cache.", m.utxo_cache_spend.load(Relaxed));
    counter!("bitcoin_utxo_cache_uncache_total", "UTXOs evicted from the cache.", m.utxo_cache_uncache.load(Relaxed));
    counter!("bitcoin_utxo_cache_add_value_total", "Cumulative satoshi value of UTXOs added to cache.", m.utxo_cache_add_value.load(Relaxed));
    counter!("bitcoin_utxo_cache_spend_value_total", "Cumulative satoshi value of UTXOs spent from cache.", m.utxo_cache_spend_value.load(Relaxed));
    counter!("bitcoin_utxo_cache_uncache_value_total", "Cumulative satoshi value of UTXOs evicted from cache.", m.utxo_cache_uncache_value.load(Relaxed));
    s
}
