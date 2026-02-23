use capnp_rpc::{rpc_twoparty_capnp, twoparty, RpcSystem};
use tokio::task::{self, JoinHandle};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::chain_capnp;
use crate::chain_capnp::chain::Client as ChainClient;
use crate::handler_capnp::handler::Client as HandlerClient;
use crate::init_capnp::init::Client as InitClient;
use crate::node_capnp::node::Client as NodeClient;
use crate::proxy_capnp::thread::Client as ThreadClient;

pub struct RpcInterface {
    rpc_handle: JoinHandle<Result<(), capnp::Error>>,
    disconnector: capnp_rpc::Disconnector<twoparty::VatId>,
    pub(crate) thread: ThreadClient,
    chain: ChainClient,
    node: NodeClient,
}

impl RpcInterface {
    pub async fn new(stream: tokio::net::UnixStream) -> Result<Self, Box<dyn std::error::Error>> {
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

    pub async fn get_height(&self) -> Result<i32, Box<dyn std::error::Error>> {
        let mut req = self.chain.get_height_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    pub async fn is_ibd(&self) -> Result<bool, Box<dyn std::error::Error>> {
        let mut req = self.chain.is_initial_block_download_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    pub async fn register_notifications(
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

    pub async fn get_verification_progress(&self) -> Result<f64, Box<dyn std::error::Error>> {
        let mut req = self.node.get_verification_progress_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    pub async fn get_mempool_size(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let mut req = self.node.get_mempool_size_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    pub async fn get_mempool_dynamic_usage(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let mut req = self.node.get_mempool_dynamic_usage_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    pub async fn get_mempool_max_usage(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let mut req = self.node.get_mempool_max_usage_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    pub async fn get_node_count(&self) -> Result<u64, Box<dyn std::error::Error>> {
        let mut req = self.node.get_node_count_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        req.get().set_flags(3); // CONNECTIONS_ALL = In | Out
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    pub async fn get_total_bytes_recv(&self) -> Result<i64, Box<dyn std::error::Error>> {
        let mut req = self.node.get_total_bytes_recv_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    pub async fn get_total_bytes_sent(&self) -> Result<i64, Box<dyn std::error::Error>> {
        let mut req = self.node.get_total_bytes_sent_request();
        req.get().get_context()?.set_thread(self.thread.clone());
        let response = req.send().promise.await?;
        Ok(response.get()?.get_result())
    }

    pub async fn disconnect(self) -> Result<(), Box<dyn std::error::Error>> {
        self.disconnector.await?;
        self.rpc_handle.await??;
        Ok(())
    }
}
