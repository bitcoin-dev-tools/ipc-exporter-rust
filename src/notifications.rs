use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;

use crate::chain_capnp::chain_notifications::{
    BlockConnectedParams, BlockConnectedResults, BlockDisconnectedParams,
    BlockDisconnectedResults, ChainStateFlushedParams, ChainStateFlushedResults, DestroyParams,
    DestroyResults, TransactionAddedToMempoolParams, TransactionAddedToMempoolResults,
    TransactionRemovedFromMempoolParams, TransactionRemovedFromMempoolResults,
    UpdatedBlockTipParams, UpdatedBlockTipResults,
};
use crate::debug;
use crate::metrics::Metrics;
use crate::DEBUG;

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
        debug!("notification: destroy");
        capnp::capability::Promise::ok(())
    }

    fn transaction_added_to_mempool(
        &mut self,
        params: TransactionAddedToMempoolParams,
        _: TransactionAddedToMempoolResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        self.metrics.mempool_tx_added.fetch_add(1, Relaxed);
        if DEBUG.load(Relaxed) {
            if let Ok(p) = params.get() {
                if let Ok(tx_data) = p.get_tx() {
                    let txid = bitcoin::consensus::deserialize::<bitcoin::Transaction>(tx_data)
                        .map(|tx| tx.compute_txid().to_string())
                        .unwrap_or_else(|_| "??".into());
                    eprintln!("mempool_add: txid={txid} size={}", tx_data.len());
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
        if DEBUG.load(Relaxed) {
            if let Ok(p) = params.get() {
                let reason = removal_reason(p.get_reason());
                if let Ok(tx_data) = p.get_tx() {
                    let txid = bitcoin::consensus::deserialize::<bitcoin::Transaction>(tx_data)
                        .map(|tx| tx.compute_txid().to_string())
                        .unwrap_or_else(|_| "??".into());
                    eprintln!(
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
                if DEBUG.load(Relaxed) {
                    let height = block.get_height();
                    let hash = block.get_hash().ok().map(display_hash).unwrap_or_default();
                    let prev = block.get_prev_hash().ok().map(display_hash).unwrap_or_default();
                    let time_max = block.get_chain_time_max();
                    eprintln!("block_connected: height={height} hash={hash} prev={prev} chain_time_max={time_max}");
                    if let Ok(role) = p.get_role() {
                        if role.get_historical() {
                            eprintln!("  (historical chainstate)");
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
        if DEBUG.load(Relaxed) {
            if let Ok(p) = params.get() {
                if let Ok(block) = p.get_block() {
                    let height = block.get_height();
                    let hash = block.get_hash().ok().map(display_hash).unwrap_or_default();
                    eprintln!("block_disconnected: height={height} hash={hash}");
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
        debug!("tip_updated");
        capnp::capability::Promise::ok(())
    }

    fn chain_state_flushed(
        &mut self,
        params: ChainStateFlushedParams,
        _: ChainStateFlushedResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        self.metrics.chain_state_flushes.fetch_add(1, Relaxed);
        if DEBUG.load(Relaxed) {
            if let Ok(p) = params.get() {
                let role = p
                    .get_role()
                    .ok()
                    .map(|r| if r.get_historical() { "historical" } else { "validated" })
                    .unwrap_or("unknown");
                let locator_len = p.get_locator().ok().map(|l| l.len()).unwrap_or(0);
                eprintln!("chain_state_flushed: role={role} locator_size={locator_len}");
            }
        }
        capnp::capability::Promise::ok(())
    }
}
