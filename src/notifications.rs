use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

use crate::chain_capnp::chain_notifications::{
    BlockConnectedParams, BlockConnectedResults, BlockDisconnectedParams,
    BlockDisconnectedResults, ChainStateFlushedParams, ChainStateFlushedResults, DestroyParams,
    DestroyResults, TransactionAddedToMempoolParams, TransactionAddedToMempoolResults,
    TransactionRemovedFromMempoolParams, TransactionRemovedFromMempoolResults,
    UpdatedBlockTipParams, UpdatedBlockTipResults,
};
use crate::metrics::Metrics;
use crate::tracing_capnp::utxo_cache_trace;

fn display_hash(bytes: &[u8]) -> String {
    bytes.iter().rev().map(|b| format!("{b:02x}")).collect()
}

fn removal_reason(reason: i32) -> &'static str {
    match reason {
        0 => "expiry",
        1 => "sizelimit",
        2 => "reorg",
        3 => "block",
        4 => "conflict",
        5 => "replaced",
        _ => "unknown",
    }
}

pub struct NotificationHandler {
    pub metrics: Arc<Metrics>,
}

impl crate::chain_capnp::chain_notifications::Server for NotificationHandler {
    fn destroy(
        &mut self,
        _: DestroyParams,
        _: DestroyResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        log::debug!("notification: destroy");
        capnp::capability::Promise::ok(())
    }

    fn transaction_added_to_mempool(
        &mut self,
        params: TransactionAddedToMempoolParams,
        _: TransactionAddedToMempoolResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        self.metrics.mempool_tx_added.fetch_add(1, Relaxed);
        if log::log_enabled!(log::Level::Debug) {
            if let Ok(p) = params.get() {
                if let Ok(tx_data) = p.get_tx() {
                    let txid = bitcoin::consensus::deserialize::<bitcoin::Transaction>(tx_data)
                        .map(|tx| tx.compute_txid().to_string())
                        .unwrap_or_else(|_| "??".into());
                    log::debug!("mempool_add: txid={txid} size={}", tx_data.len());
                }
            }
        }
        capnp::capability::Promise::ok(())
    }

    fn transaction_removed_from_mempool(
        &mut self,
        params: TransactionRemovedFromMempoolParams,
        _: TransactionRemovedFromMempoolResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        self.metrics.mempool_tx_removed.fetch_add(1, Relaxed);
        if log::log_enabled!(log::Level::Debug) {
            if let Ok(p) = params.get() {
                let reason = removal_reason(p.get_reason());
                if let Ok(tx_data) = p.get_tx() {
                    let txid = bitcoin::consensus::deserialize::<bitcoin::Transaction>(tx_data)
                        .map(|tx| tx.compute_txid().to_string())
                        .unwrap_or_else(|_| "??".into());
                    log::debug!(
                        "mempool_remove: txid={txid} reason={reason} size={}",
                        tx_data.len()
                    );
                }
            }
        }
        capnp::capability::Promise::ok(())
    }

    fn block_connected(
        &mut self,
        params: BlockConnectedParams,
        _: BlockConnectedResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        self.metrics.blocks_connected.fetch_add(1, Relaxed);
        if let Ok(p) = params.get() {
            if let Ok(block) = p.get_block() {
                self.metrics.block_height.store(block.get_height(), Relaxed);
                if log::log_enabled!(log::Level::Debug) {
                    let height = block.get_height();
                    let hash = block.get_hash().ok().map(display_hash).unwrap_or_default();
                    let prev = block.get_prev_hash().ok().map(display_hash).unwrap_or_default();
                    let time_max = block.get_chain_time_max();
                    log::debug!("block_connected: height={height} hash={hash} prev={prev} chain_time_max={time_max}");
                    if let Ok(role) = p.get_role() {
                        if role.get_historical() {
                            log::debug!("  (historical chainstate)");
                        }
                    }
                }
            }
        }
        capnp::capability::Promise::ok(())
    }

    fn block_disconnected(
        &mut self,
        params: BlockDisconnectedParams,
        _: BlockDisconnectedResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        self.metrics.blocks_disconnected.fetch_add(1, Relaxed);
        if log::log_enabled!(log::Level::Debug) {
            if let Ok(p) = params.get() {
                if let Ok(block) = p.get_block() {
                    let height = block.get_height();
                    let hash = block.get_hash().ok().map(display_hash).unwrap_or_default();
                    log::debug!("block_disconnected: height={height} hash={hash}");
                }
            }
        }
        capnp::capability::Promise::ok(())
    }

    fn updated_block_tip(
        &mut self,
        _: UpdatedBlockTipParams,
        _: UpdatedBlockTipResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        self.metrics.tip_updates.fetch_add(1, Relaxed);
        log::debug!("tip_updated");
        capnp::capability::Promise::ok(())
    }

    fn chain_state_flushed(
        &mut self,
        params: ChainStateFlushedParams,
        _: ChainStateFlushedResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        self.metrics.chain_state_flushes.fetch_add(1, Relaxed);
        if log::log_enabled!(log::Level::Debug) {
            if let Ok(p) = params.get() {
                let role = p
                    .get_role()
                    .ok()
                    .map(|r| if r.get_historical() { "historical" } else { "validated" })
                    .unwrap_or("unknown");
                let locator_len = p.get_locator().ok().map(|l| l.len()).unwrap_or(0);
                log::debug!("chain_state_flushed: role={role} locator_size={locator_len}");
            }
        }
        capnp::capability::Promise::ok(())
    }
}

pub struct UtxoCacheHandler {
    pub metrics: Arc<Metrics>,
}

impl utxo_cache_trace::Server for UtxoCacheHandler {
    fn destroy(
        &mut self,
        _: utxo_cache_trace::DestroyParams,
        _: utxo_cache_trace::DestroyResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        log::debug!("utxo_cache_trace: destroy");
        capnp::capability::Promise::ok(())
    }

    fn add(
        &mut self,
        params: utxo_cache_trace::AddParams,
        _: utxo_cache_trace::AddResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        self.metrics.utxo_cache_add.fetch_add(1, Relaxed);
        if let Ok(p) = params.get() {
            if let Ok(info) = p.get_utxo_info() {
                self.metrics
                    .utxo_cache_add_value
                    .fetch_add(info.get_value() as u64, Relaxed);
                if log::log_enabled!(log::Level::Debug) {
                    let hash = info.get_outpoint_hash().ok().map(display_hash).unwrap_or_default();
                    log::debug!(
                        "utxo_cache_add: outpoint={hash}:{} height={} value={} coinbase={}",
                        info.get_outpoint_n(),
                        info.get_height(),
                        info.get_value(),
                        info.get_is_coinbase()
                    );
                }
            }
        }
        capnp::capability::Promise::ok(())
    }

    fn spend(
        &mut self,
        params: utxo_cache_trace::SpendParams,
        _: utxo_cache_trace::SpendResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        self.metrics.utxo_cache_spend.fetch_add(1, Relaxed);
        if let Ok(p) = params.get() {
            if let Ok(info) = p.get_utxo_info() {
                self.metrics
                    .utxo_cache_spend_value
                    .fetch_add(info.get_value() as u64, Relaxed);
                if log::log_enabled!(log::Level::Debug) {
                    let hash = info.get_outpoint_hash().ok().map(display_hash).unwrap_or_default();
                    log::debug!(
                        "utxo_cache_spend: outpoint={hash}:{} height={} value={} coinbase={}",
                        info.get_outpoint_n(),
                        info.get_height(),
                        info.get_value(),
                        info.get_is_coinbase()
                    );
                }
            }
        }
        capnp::capability::Promise::ok(())
    }

    fn uncache(
        &mut self,
        params: utxo_cache_trace::UncacheParams,
        _: utxo_cache_trace::UncacheResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        self.metrics.utxo_cache_uncache.fetch_add(1, Relaxed);
        if let Ok(p) = params.get() {
            if let Ok(info) = p.get_utxo_info() {
                self.metrics
                    .utxo_cache_uncache_value
                    .fetch_add(info.get_value() as u64, Relaxed);
                if log::log_enabled!(log::Level::Debug) {
                    let hash = info.get_outpoint_hash().ok().map(display_hash).unwrap_or_default();
                    log::debug!(
                        "utxo_cache_uncache: outpoint={hash}:{} height={} value={} coinbase={}",
                        info.get_outpoint_n(),
                        info.get_height(),
                        info.get_value(),
                        info.get_is_coinbase()
                    );
                }
            }
        }
        capnp::capability::Promise::ok(())
    }
}
