use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::metrics::{format_metrics, Metrics};

pub async fn serve_metrics(metrics: Arc<Metrics>, addr: String) {
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind metrics server on {addr}: {e}"));
    eprintln!("metrics server listening on http://{addr}/metrics");
    loop {
        let Ok((mut stream, _)) = listener.accept().await else {
            continue;
        };
        let metrics = metrics.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
            let body = format_metrics(&metrics);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\r\n{body}",
                body.len(),
            );
            let _ = stream.write_all(response.as_bytes()).await;
        });
    }
}
