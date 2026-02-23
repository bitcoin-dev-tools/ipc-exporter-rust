mod metrics;
mod rpc;
mod server;

use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::Arc;
use std::{env, time::Duration};
use tokio::task;

use metrics::Metrics;
use rpc::RpcInterface;

pub(crate) static DEBUG: AtomicBool = AtomicBool::new(false);

macro_rules! debug {
    ($($arg:tt)*) => {
        if crate::DEBUG.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!($($arg)*);
        }
    };
}
pub(crate) use debug;

#[allow(dead_code)]
pub(crate) mod chain_capnp {
    include!(concat!(env!("OUT_DIR"), "/chain_capnp.rs"));
}
#[allow(dead_code)]
pub(crate) mod common_capnp {
    include!(concat!(env!("OUT_DIR"), "/common_capnp.rs"));
}
#[allow(dead_code)]
pub(crate) mod echo_capnp {
    include!(concat!(env!("OUT_DIR"), "/echo_capnp.rs"));
}
#[allow(dead_code)]
pub(crate) mod handler_capnp {
    include!(concat!(env!("OUT_DIR"), "/handler_capnp.rs"));
}
#[allow(dead_code)]
pub(crate) mod init_capnp {
    include!(concat!(env!("OUT_DIR"), "/init_capnp.rs"));
}
#[allow(dead_code)]
pub(crate) mod mining_capnp {
    include!(concat!(env!("OUT_DIR"), "/mining_capnp.rs"));
}
#[allow(dead_code)]
pub(crate) mod node_capnp {
    include!(concat!(env!("OUT_DIR"), "/node_capnp.rs"));
}
#[allow(dead_code)]
pub(crate) mod wallet_capnp {
    include!(concat!(env!("OUT_DIR"), "/wallet_capnp.rs"));
}
#[allow(dead_code)]
pub(crate) mod proxy_capnp {
    include!(concat!(env!("OUT_DIR"), "/mp/proxy_capnp.rs"));
}

use chain_capnp::chain_notifications::{
    BlockConnectedParams, BlockConnectedResults, BlockDisconnectedParams,
    BlockDisconnectedResults, ChainStateFlushedParams, ChainStateFlushedResults, DestroyParams,
    DestroyResults, TransactionAddedToMempoolParams, TransactionAddedToMempoolResults,
    TransactionRemovedFromMempoolParams, TransactionRemovedFromMempoolResults,
    UpdatedBlockTipParams, UpdatedBlockTipResults,
};

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

struct NotificationHandler {
    metrics: Arc<Metrics>,
}

impl chain_capnp::chain_notifications::Server for NotificationHandler {
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

async fn poll_metrics(rpc: &RpcInterface, metrics: &Metrics) -> Result<(), Box<dyn std::error::Error>> {
    let h = rpc.get_height().await?;
    let ibd = rpc.is_ibd().await?;
    let progress = rpc.get_verification_progress().await?;
    let mempool_size = rpc.get_mempool_size().await?;
    let mempool_bytes = rpc.get_mempool_dynamic_usage().await?;
    let mempool_max = rpc.get_mempool_max_usage().await?;
    let peers = rpc.get_node_count().await?;
    let bytes_recv = rpc.get_total_bytes_recv().await?;
    let bytes_sent = rpc.get_total_bytes_sent().await?;

    metrics.chain_height.store(h, Relaxed);
    metrics.ibd.store(ibd, Relaxed);
    metrics.verification_progress.store(progress.to_bits(), Relaxed);
    metrics.mempool_size.store(mempool_size, Relaxed);
    metrics.mempool_bytes.store(mempool_bytes, Relaxed);
    metrics.mempool_max.store(mempool_max, Relaxed);
    metrics.peers.store(peers, Relaxed);
    metrics.bytes_recv.store(bytes_recv, Relaxed);
    metrics.bytes_sent.store(bytes_sent, Relaxed);

    debug!("--- poll ---");
    debug!("  chain_height={h} ibd={ibd} verification_progress={progress:.6}");
    debug!("  block_height={}", metrics.block_height.load(Relaxed));
    debug!("  mempool_size={mempool_size} mempool_bytes={mempool_bytes} mempool_max={mempool_max}");
    debug!("  peers={peers} bytes_recv={bytes_recv} bytes_sent={bytes_sent}");
    debug!(
        "  blocks_connected={} blocks_disconnected={}",
        metrics.blocks_connected.load(Relaxed),
        metrics.blocks_disconnected.load(Relaxed)
    );
    debug!(
        "  mempool_tx_added={} mempool_tx_removed={}",
        metrics.mempool_tx_added.load(Relaxed),
        metrics.mempool_tx_removed.load(Relaxed)
    );
    debug!(
        "  tip_updates={} chain_state_flushes={}",
        metrics.tip_updates.load(Relaxed),
        metrics.chain_state_flushes.load(Relaxed)
    );
    Ok(())
}

async fn run(stream: tokio::net::UnixStream, metrics_addr: String) -> Result<(), Box<dyn std::error::Error>> {
    let rpc = RpcInterface::new(stream).await?;
    let metrics = Metrics::new();
    tokio::spawn(server::serve_metrics(metrics.clone(), metrics_addr));

    let subscription = rpc
        .register_notifications(NotificationHandler {
            metrics: metrics.clone(),
        })
        .await?;
    eprintln!("registered for chain notifications");

    poll_metrics(&rpc, &metrics).await?;
    eprintln!(
        "chain height: {}, IBD: {}",
        metrics.chain_height.load(Relaxed),
        metrics.ibd.load(Relaxed)
    );

    let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(60)) => {
                poll_metrics(&rpc, &metrics).await?;
            }
            _ = &mut ctrl_c => {
                eprintln!("received SIGINT, shutting down...");
                break;
            }
            _ = sigterm.recv() => {
                eprintln!("received SIGTERM, shutting down...");
                break;
            }
        }
    }

    let mut req = subscription.disconnect_request();
    req.get().get_context()?.set_thread(rpc.thread.clone());
    req.send().promise.await?;
    eprintln!("notifications disconnected");

    rpc.disconnect().await?;
    eprintln!("RPC disconnected");
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut debug = false;
    let mut metrics_addr = "127.0.0.1:9332".to_string();
    let mut socket_path = None;
    let mut args_iter = env::args().skip(1);
    while let Some(arg) = args_iter.next() {
        match arg.as_str() {
            "--debug" => debug = true,
            "--metrics-addr" => {
                metrics_addr = args_iter.next().unwrap_or_else(|| {
                    eprintln!("--metrics-addr requires a value");
                    std::process::exit(1);
                });
            }
            _ => socket_path = Some(arg),
        }
    }
    let Some(socket_path) = socket_path else {
        eprintln!("usage: ipc-exporter-rust [--debug] [--metrics-addr HOST:PORT] <socket-path>");
        std::process::exit(1);
    };
    DEBUG.store(debug, Relaxed);

    let stream = tokio::net::UnixStream::connect(&socket_path).await?;
    eprintln!("connected to {socket_path}");

    task::LocalSet::new().run_until(run(stream, metrics_addr)).await
}
