use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
use tokio::task::{self, JoinHandle};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

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
use handler_capnp::handler::Client as HandlerClient;
use chain_capnp::chain_notifications::{
    BlockConnectedParams, BlockConnectedResults, BlockDisconnectedParams,
    BlockDisconnectedResults, ChainStateFlushedParams, ChainStateFlushedResults, DestroyParams,
    DestroyResults, TransactionAddedToMempoolParams, TransactionAddedToMempoolResults,
    TransactionRemovedFromMempoolParams, TransactionRemovedFromMempoolResults,
    UpdatedBlockTipParams, UpdatedBlockTipResults,
};
use init_capnp::init::Client as InitClient;
use proxy_capnp::thread::Client as ThreadClient;

struct RpcInterface {
    rpc_handle: JoinHandle<Result<(), capnp::Error>>,
    disconnector: capnp_rpc::Disconnector<twoparty::VatId>,
    thread: ThreadClient,
    chain: ChainClient,
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

        eprintln!("IPC handshake complete");
        Ok(Self {
            rpc_handle,
            disconnector,
            thread,
            chain,
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
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client: chain_capnp::chain_notifications::Client = capnp_rpc::new_client(handler);
        let mut req = self.chain.handle_notifications_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        req.get().set_notifications(client);
        let _response = req.send().promise.await?;
        Ok(())
    }

    async fn disconnect(self) -> Result<(), Box<dyn std::error::Error>> {
        self.disconnector.await?;
        self.rpc_handle.await??;
        Ok(())
    }
}

struct NotificationHandler;

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
        _: TransactionAddedToMempoolParams,
        _: TransactionAddedToMempoolResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        eprintln!("notification: tx added to mempool");
        capnp::capability::Promise::ok(())
    }

    fn transaction_removed_from_mempool(
        &mut self,
        _: TransactionRemovedFromMempoolParams,
        _: TransactionRemovedFromMempoolResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        eprintln!("notification: tx removed from mempool");
        capnp::capability::Promise::ok(())
    }

    fn block_connected(
        &mut self,
        params: BlockConnectedParams,
        _: BlockConnectedResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        let height = match params.get() {
            Ok(p) => match p.get_block() {
                Ok(block) => block.get_height(),
                Err(_) => -1,
            },
            Err(_) => -1,
        };
        eprintln!("notification: block connected at height {height}");
        capnp::capability::Promise::ok(())
    }

    fn block_disconnected(
        &mut self,
        params: BlockDisconnectedParams,
        _: BlockDisconnectedResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        let height = match params.get() {
            Ok(p) => match p.get_block() {
                Ok(block) => block.get_height(),
                Err(_) => -1,
            },
            Err(_) => -1,
        };
        eprintln!("notification: block disconnected at height {height}");
        capnp::capability::Promise::ok(())
    }

    fn updated_block_tip(
        &mut self,
        _: UpdatedBlockTipParams,
        _: UpdatedBlockTipResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        eprintln!("notification: block tip updated");
        capnp::capability::Promise::ok(())
    }

    fn chain_state_flushed(
        &mut self,
        _: ChainStateFlushedParams,
        _: ChainStateFlushedResults,
    ) -> capnp::capability::Promise<(), capnp::Error> {
        eprintln!("notification: chain state flushed");
        capnp::capability::Promise::ok(())
    }
}

async fn run(stream: tokio::net::UnixStream) -> Result<(), Box<dyn std::error::Error>> {
    let rpc = RpcInterface::new(stream).await?;

    let height = rpc.get_height().await?;
    let ibd = rpc.is_ibd().await?;
    eprintln!("chain height: {height}, IBD: {ibd}");

    rpc.register_notifications(NotificationHandler).await?;
    eprintln!("registered for chain notifications, waiting for events...");

    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let h = rpc.get_height().await?;
        eprintln!("poll: chain height {h}");
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
