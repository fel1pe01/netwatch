#[cfg(test)]
mod tests {
    use netwatch::discovery::interfaces::discover_interfaces;
    use netwatch::discovery::routing::discover_routes;

    #[tokio::test]
    async fn test_discover_interfaces_non_empty() {
        let result = discover_interfaces().await;
        assert!(result.is_ok());
        let ifaces = result.unwrap();
        assert!(
            !ifaces.is_empty(),
            "Expected at least one interface (loopback)"
        );
    }

    #[tokio::test]
    async fn test_discover_routes_non_empty() {
        let result = discover_routes().await;
        assert!(result.is_ok());
        let routes = result.unwrap();
        assert!(!routes.is_empty(), "Expected at least one route (default)");
    }
}
