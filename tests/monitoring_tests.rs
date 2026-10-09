#[cfg(test)]
mod tests {
    use netwatch::monitoring::{check_tcp, ping};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_check_tcp_success() {
        // Spin up a local listener
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Spawn a task that accepts and keeps connection
        tokio::spawn(async move {
            let (_socket, _) = listener.accept().await.unwrap();
        });

        let res = check_tcp(&addr.ip().to_string(), addr.port(), 2000)
            .await
            .unwrap();
        assert!(res.success);
        assert!(res.latency_ms.is_some());
        assert!(res.error.is_none());
    }

    #[tokio::test]
    async fn test_check_tcp_closed_port() {
        let _listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let res = check_tcp("127.0.0.1", 54321, 100).await.unwrap();
        assert!(!res.success);
        assert!(res.latency_ms.is_none());
        assert!(res.error.is_some());
    }

    #[tokio::test]
    async fn test_pings_loopback() {
        let res = ping("127.0.0.1", 200).await;
        match res {
            Ok(hc) => {
                assert!(hc.success);
            }
            Err(_) => {
                println!("ping not available, skipping test");
            }
        }
    }
}
