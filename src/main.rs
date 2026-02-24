mod metrics;
mod notifications;
mod rpc;
mod server;

use anyhow::Context;
use log::{debug, info};
use std::sync::atomic::Ordering::Relaxed;
use std::{env, time::Duration};
use tokio::task;

use metrics::Metrics;
use notifications::{NotificationHandler, UtxoCacheHandler};
use rpc::RpcInterface;

#[allow(dead_code, unused_parens, clippy::all)]
pub(crate) mod chain_capnp {
    include!(concat!(env!("OUT_DIR"), "/chain_capnp.rs"));
}
#[allow(dead_code, unused_parens, clippy::all)]
pub(crate) mod common_capnp {
    include!(concat!(env!("OUT_DIR"), "/common_capnp.rs"));
}
#[allow(dead_code, unused_parens, clippy::all)]
pub(crate) mod echo_capnp {
    include!(concat!(env!("OUT_DIR"), "/echo_capnp.rs"));
}
#[allow(dead_code, unused_parens, clippy::all)]
pub(crate) mod handler_capnp {
    include!(concat!(env!("OUT_DIR"), "/handler_capnp.rs"));
}
#[allow(dead_code, unused_parens, clippy::all)]
pub(crate) mod init_capnp {
    include!(concat!(env!("OUT_DIR"), "/init_capnp.rs"));
}
#[allow(dead_code, unused_parens, clippy::all)]
pub(crate) mod mining_capnp {
    include!(concat!(env!("OUT_DIR"), "/mining_capnp.rs"));
}
#[allow(dead_code, unused_parens, clippy::all)]
pub(crate) mod node_capnp {
    include!(concat!(env!("OUT_DIR"), "/node_capnp.rs"));
}
#[allow(dead_code, unused_parens, clippy::all)]
pub(crate) mod tracing_capnp {
    include!(concat!(env!("OUT_DIR"), "/tracing_capnp.rs"));
}
#[allow(dead_code, unused_parens, clippy::all)]
pub(crate) mod wallet_capnp {
    include!(concat!(env!("OUT_DIR"), "/wallet_capnp.rs"));
}
#[allow(dead_code, unused_parens, clippy::all)]
pub(crate) mod proxy_capnp {
    include!(concat!(env!("OUT_DIR"), "/mp/proxy_capnp.rs"));
}

async fn poll_metrics(rpc: &RpcInterface, metrics: &Metrics) -> anyhow::Result<()> {
    let header_height = rpc.get_header_tip().await?;
    let ibd = rpc.is_ibd().await?;
    let progress = rpc.get_verification_progress().await?;
    let mempool_size = rpc.get_mempool_size().await?;
    let mempool_bytes = rpc.get_mempool_dynamic_usage().await?;
    let mempool_max = rpc.get_mempool_max_usage().await?;
    let peers = rpc.get_node_count().await?;
    let bytes_recv = rpc.get_total_bytes_recv().await?;
    let bytes_sent = rpc.get_total_bytes_sent().await?;

    if let Some(h) = header_height {
        metrics.header_height.store(h, Relaxed);
    }
    metrics.ibd.store(ibd, Relaxed);
    metrics.verification_progress.store(progress.to_bits(), Relaxed);
    metrics.mempool_size.store(mempool_size, Relaxed);
    metrics.mempool_bytes.store(mempool_bytes, Relaxed);
    metrics.mempool_max.store(mempool_max, Relaxed);
    metrics.peers.store(peers, Relaxed);
    metrics.bytes_recv.store(bytes_recv, Relaxed);
    metrics.bytes_sent.store(bytes_sent, Relaxed);

    debug!("poll: header_height={header_height:?} ibd={ibd} verification_progress={progress:.6}");
    debug!("poll: block_height={}", metrics.block_height.load(Relaxed));
    debug!("poll: mempool_size={mempool_size} mempool_bytes={mempool_bytes} mempool_max={mempool_max}");
    debug!("poll: peers={peers} bytes_recv={bytes_recv} bytes_sent={bytes_sent}");
    debug!(
        "poll: blocks_connected={} blocks_disconnected={}",
        metrics.blocks_connected.load(Relaxed),
        metrics.blocks_disconnected.load(Relaxed)
    );
    debug!(
        "poll: mempool_tx_added={} mempool_tx_removed={}",
        metrics.mempool_tx_added.load(Relaxed),
        metrics.mempool_tx_removed.load(Relaxed)
    );
    debug!(
        "poll: tip_updates={} chain_state_flushes={}",
        metrics.tip_updates.load(Relaxed),
        metrics.chain_state_flushes.load(Relaxed)
    );
    debug!(
        "poll: utxo_cache_add={} utxo_cache_spend={} utxo_cache_uncache={}",
        metrics.utxo_cache_add.load(Relaxed),
        metrics.utxo_cache_spend.load(Relaxed),
        metrics.utxo_cache_uncache.load(Relaxed)
    );
    Ok(())
}

async fn run(stream: tokio::net::UnixStream, metrics_addr: String) -> anyhow::Result<()> {
    let rpc = RpcInterface::new(stream).await?;
    let metrics = Metrics::new();
    tokio::spawn(server::serve_metrics(metrics.clone(), metrics_addr));

    let subscription = rpc
        .register_notifications(NotificationHandler {
            metrics: metrics.clone(),
        })
        .await?;
    info!("registered for chain notifications");

    let utxo_trace = rpc
        .register_utxo_cache_trace(UtxoCacheHandler {
            metrics: metrics.clone(),
        })
        .await?;
    info!("registered for UTXO cache trace");

    poll_metrics(&rpc, &metrics).await?;
    info!(
        "header height: {}, IBD: {}",
        metrics.header_height.load(Relaxed),
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
                info!("received SIGINT, shutting down...");
                break;
            }
            _ = sigterm.recv() => {
                info!("received SIGTERM, shutting down...");
                break;
            }
        }
    }

    let mut req = subscription.disconnect_request();
    req.get().get_context()?.set_thread(rpc.thread.clone());
    req.send().promise.await?;

    let mut req = utxo_trace.disconnect_request();
    req.get().get_context()?.set_thread(rpc.thread.clone());
    req.send().promise.await?;

    info!("notifications disconnected");

    rpc.disconnect().await?;
    info!("RPC disconnected");
    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
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
            s if s.starts_with("--") => {
                eprintln!("unknown flag: {s}");
                std::process::exit(1);
            }
            _ => socket_path = Some(arg),
        }
    }
    let Some(socket_path) = socket_path else {
        eprintln!("usage: ipc-exporter-rust [--debug] [--metrics-addr HOST:PORT] <socket-path>");
        std::process::exit(1);
    };
    env_logger::Builder::new()
        .filter_level(if debug {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        })
        .format_target(false)
        .init();

    let stream = tokio::net::UnixStream::connect(&socket_path)
        .await
        .context("failed to connect to IPC socket")?;
    info!("connected to {socket_path}");

    task::LocalSet::new().run_until(run(stream, metrics_addr)).await
}
