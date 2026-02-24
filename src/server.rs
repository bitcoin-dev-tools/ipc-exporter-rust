use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use crate::metrics::{format_metrics, Metrics};

pub async fn serve_metrics(metrics: Arc<Metrics>, addr: String, cancel: CancellationToken) {
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind metrics server on {addr}: {e}"));
    log::info!("metrics server listening on http://{addr}/metrics");
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            result = listener.accept() => {
                let Ok((mut stream, _)) = result else {
                    continue;
                };
                let metrics = metrics.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let request = std::str::from_utf8(&buf[..n]).unwrap_or("");
                    let is_metrics = request
                        .lines()
                        .next()
                        .is_some_and(|line| line.starts_with("GET /metrics"));

                    if is_metrics {
                        let body = format_metrics(&metrics);
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\r\n{body}",
                            body.len(),
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                    } else {
                        let _ = stream
                            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                            .await;
                    }
                });
            }
        }
    }
    log::info!("metrics server stopped");
}
