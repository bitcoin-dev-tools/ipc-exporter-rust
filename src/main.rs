use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
use tokio::task::{self, JoinHandle};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use std::sync::atomic::{AtomicI32, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::{env, time::Duration};

#[allow(dead_code)]
mod chain_capnp {
    include!(concat!(env!("OUT_DIR"), "/chain_capnp.rs"));
}
#[allow(dead_code)]
mod common_capnp {
    include!(concat!(env!("OUT_DIR"), "/common_capnp.rs"));
}
#[allow(dead_code)]
mod echo_capnp {
    include!(concat!(env!("OUT_DIR"), "/echo_capnp.rs"));
}
#[allow(dead_code)]
mod handler_capnp {
    include!(concat!(env!("OUT_DIR"), "/handler_capnp.rs"));
}
#[allow(dead_code)]
mod init_capnp {
    include!(concat!(env!("OUT_DIR"), "/init_capnp.rs"));
}
#[allow(dead_code)]
mod mining_capnp {
    include!(concat!(env!("OUT_DIR"), "/mining_capnp.rs"));
}
#[allow(dead_code)]
mod node_capnp {
    include!(concat!(env!("OUT_DIR"), "/node_capnp.rs"));
}
#[allow(dead_code)]
mod wallet_capnp {
    include!(concat!(env!("OUT_DIR"), "/wallet_capnp.rs"));
}
#[allow(dead_code)]
mod proxy_capnp {
    include!(concat!(env!("OUT_DIR"), "/mp/proxy_capnp.rs"));
}

use chain_capnp::chain::Client as ChainClient;
use chain_capnp::chain_notifications::{
    BlockConnectedParams, BlockConnectedResults, BlockDisconnectedParams,
    BlockDisconnectedResults, ChainStateFlushedParams, ChainStateFlushedResults, DestroyParams,
    DestroyResults, TransactionAddedToMempoolParams, TransactionAddedToMempoolResults,
    TransactionRemovedFromMempoolParams, TransactionRemovedFromMempoolResults,
    UpdatedBlockTipParams, UpdatedBlockTipResults,
};
use handler_capnp::handler::Client as HandlerClient;
use init_capnp::init::Client as InitClient;
use node_capnp::node::Client as NodeClient;
use proxy_capnp::thread::Client as ThreadClient;

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

struct Metrics {
    blocks_connected: AtomicU64,
    blocks_disconnected: AtomicU64,
    mempool_tx_added: AtomicU64,
    mempool_tx_removed: AtomicU64,
    tip_updates: AtomicU64,
    chain_state_flushes: AtomicU64,
    block_height: AtomicI32,
}

impl Metrics {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            blocks_connected: AtomicU64::new(0),
            blocks_disconnected: AtomicU64::new(0),
            mempool_tx_added: AtomicU64::new(0),
            mempool_tx_removed: AtomicU64::new(0),
            tip_updates: AtomicU64::new(0),
            chain_state_flushes: AtomicU64::new(0),
            block_height: AtomicI32::new(-1),
        })
    }
}

struct RpcInterface {
    rpc_handle: JoinHandle<Result<(), capnp::Error>>,
    disconnector: capnp_rpc::Disconnector<twoparty::VatId>,
    thread: ThreadClient,
    chain: ChainClient,
    node: NodeClient,
}

impl RpcInterface {
    async fn new(stream: tokio::net::UnixStream) -> Result<Self, Box<dyn std::error::Error>> {
        let (reader, writer) = stream.into_split();
        let network = Box::new(twoparty::VatNetwork::new(
            reader.compat(),
            writer.compat_write(),
            rpc_twoparty_capnp::Side::Client,
            Default::default(),
        ));

        let mut rpc = RpcSystem::new(network, None);
        let init: InitClient = rpc.bootstrap(rpc_twoparty_capnp::Side::Server);
        let disconnector = rpc.get_disconnector();
        let rpc_handle = task::spawn_local(rpc);

        let response = init.construct_request().send().promise.await?;
        let thread_map = response.get()?.get_thread_map()?;

        let response = thread_map.make_thread_request().send().promise.await?;
        let thread = response.get()?.get_result()?;

        let mut req = init.make_chain_request();
        req.get().get_context()?.set_thread(thread.clone());
        let response = req.send().promise.await?;
        let chain = response.get()?.get_result()?;

        let mut req = init.make_node_request();
        req.get().get_context()?.set_thread(thread.clone());
        let response = req.send().promise.await?;
        let node = response.get()?.get_result()?;

        eprintln!("IPC handshake complete");
        Ok(Self {
            rpc_handle,
            disconnector,
            thread,
            chain,
            node,
        })
    }

    async fn get_height(&self) -> Result<i32, Box<dyn std::error::Error>> {
        let mut req = self.chain.get_height_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    async fn is_ibd(&self) -> Result<bool, Box<dyn std::error::Error>> {
        let mut req = self.chain.is_initial_block_download_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    async fn register_notifications(
        &self,
        handler: impl chain_capnp::chain_notifications::Server + 'static,
    ) -> Result<HandlerClient, Box<dyn std::error::Error>> {
        let client: chain_capnp::chain_notifications::Client = capnp_rpc::new_client(handler);
        let mut req = self.chain.handle_notifications_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        req.get().set_notifications(client);
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result()?)
    }

    async fn get_verification_progress(&self) -> Result<f64, Box<dyn std::error::Error>> {
        let mut req = self.node.get_verification_progress_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    async fn get_mempool_size(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let mut req = self.node.get_mempool_size_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    async fn get_mempool_dynamic_usage(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let mut req = self.node.get_mempool_dynamic_usage_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    async fn get_mempool_max_usage(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let mut req = self.node.get_mempool_max_usage_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    async fn get_node_count(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let mut req = self.node.get_node_count_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        req.get().set_flags(3); // CONNECTIONS_ALL = In | Out
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    async fn get_total_bytes_recv(&self) -> Result<i64, Box<dyn std::error::Error>> {
        let mut req = self.node.get_total_bytes_recv_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    async fn get_total_bytes_sent(&self) -> Result<i64, Box<dyn std::error::Error>> {
        let mut req = self.node.get_total_bytes_sent_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    async fn disconnect(self) -> Result<(), Box<dyn std::error::Error>> {
        self.disconnector.await?;
        self.rpc_handle.await??;
        Ok(())
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
        eprintln!("notification: destroy");
        capnp::capability::Promise::ok(())
    }

    fn transaction_added_to_mempool(
        &mut self,
        params: TransactionAddedToMempoolParams,
        _: TransactionAddedToMempoolResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        let total = self.metrics.mempool_tx_added.fetch_add(1, Relaxed) + 1;
        if let Ok(p) = params.get() {
            if let Ok(tx_data) = p.get_tx() {
                let txid = bitcoin::consensus::deserialize::<bitcoin::Transaction>(tx_data)
                    .map(|tx| tx.compute_txid().to_string())
                    .unwrap_or_else(|_| "??".into());
                eprintln!("mempool_add: txid={txid} size={} total={total}", tx_data.len());
            }
        }
        capnp::capability::Promise::ok(())
    }

    fn transaction_removed_from_mempool(
        &mut self,
        params: TransactionRemovedFromMempoolParams,
        _: TransactionRemovedFromMempoolResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        let total = self.metrics.mempool_tx_removed.fetch_add(1, Relaxed) + 1;
        if let Ok(p) = params.get() {
            let reason = removal_reason(p.get_reason());
            if let Ok(tx_data) = p.get_tx() {
                let txid = bitcoin::consensus::deserialize::<bitcoin::Transaction>(tx_data)
                    .map(|tx| tx.compute_txid().to_string())
                    .unwrap_or_else(|_| "??".into());
                eprintln!(
                    "mempool_remove: txid={txid} reason={reason} size={} total={total}",
                    tx_data.len()
                );
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
                let height = block.get_height();
                self.metrics.block_height.store(height, Relaxed);
                let hash = block.get_hash().ok().map(display_hash).unwrap_or_default();
                let prev = block.get_prev_hash().ok().map(display_hash).unwrap_or_default();
                let time_max = block.get_chain_time_max();
                eprintln!("block_connected: height={height} hash={hash} prev={prev} chain_time_max={time_max}");
            }
            if let Ok(role) = p.get_role() {
                if role.get_historical() {
                    eprintln!("  (historical chainstate)");
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
        if let Ok(p) = params.get() {
            if let Ok(block) = p.get_block() {
                let height = block.get_height();
                let hash = block.get_hash().ok().map(display_hash).unwrap_or_default();
                eprintln!("block_disconnected: height={height} hash={hash}");
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
        eprintln!("tip_updated");
        capnp::capability::Promise::ok(())
    }

    fn chain_state_flushed(
        &mut self,
        params: ChainStateFlushedParams,
        _: ChainStateFlushedResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        self.metrics.chain_state_flushes.fetch_add(1, Relaxed);
        if let Ok(p) = params.get() {
            let role = p
                .get_role()
                .ok()
                .map(|r| if r.get_historical() { "historical" } else { "validated" })
                .unwrap_or("unknown");
            let locator_len = p.get_locator().ok().map(|l| l.len()).unwrap_or(0);
            eprintln!("chain_state_flushed: role={role} locator_size={locator_len}");
        }
        capnp::capability::Promise::ok(())
    }
}

async fn run(stream: tokio::net::UnixStream) -> Result<(), Box<dyn std::error::Error>> {
    let rpc = RpcInterface::new(stream).await?;

    let height = rpc.get_height().await?;
    let ibd = rpc.is_ibd().await?;
    eprintln!("chain height: {height}, IBD: {ibd}");

    let metrics = Metrics::new();
    let _subscription = rpc
        .register_notifications(NotificationHandler {
            metrics: metrics.clone(),
        })
        .await?;
    eprintln!("registered for chain notifications, waiting for events...");

    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let h = rpc.get_height().await?;
        let ibd = rpc.is_ibd().await?;
        let progress = rpc.get_verification_progress().await?;
        let mempool_size = rpc.get_mempool_size().await?;
        let mempool_bytes = rpc.get_mempool_dynamic_usage().await?;
        let mempool_max = rpc.get_mempool_max_usage().await?;
        let peers = rpc.get_node_count().await?;
        let bytes_recv = rpc.get_total_bytes_recv().await?;
        let bytes_sent = rpc.get_total_bytes_sent().await?;

        eprintln!("--- poll ---");
        eprintln!("  chain_height={h} ibd={ibd} verification_progress={progress:.6}");
        eprintln!("  block_height={}", metrics.block_height.load(Relaxed));
        eprintln!("  mempool_size={mempool_size} mempool_bytes={mempool_bytes} mempool_max={mempool_max}");
        eprintln!("  peers={peers} bytes_recv={bytes_recv} bytes_sent={bytes_sent}");
        eprintln!(
            "  blocks_connected={} blocks_disconnected={}",
            metrics.blocks_connected.load(Relaxed),
            metrics.blocks_disconnected.load(Relaxed)
        );
        eprintln!(
            "  mempool_tx_added={} mempool_tx_removed={}",
            metrics.mempool_tx_added.load(Relaxed),
            metrics.mempool_tx_removed.load(Relaxed)
        );
        eprintln!(
            "  tip_updates={} chain_state_flushes={}",
            metrics.tip_updates.load(Relaxed),
            metrics.chain_state_flushes.load(Relaxed)
        );
    }

    #[allow(unreachable_code)]
    {
        rpc.disconnect().await?;
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("usage: {} <socket-path>", args[0]);
        std::process::exit(1);
    }

    let stream = tokio::net::UnixStream::connect(&args[1]).await?;
    eprintln!("connected to {}", args[1]);

    task::LocalSet::new().run_until(run(stream)).await
}
