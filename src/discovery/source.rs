use crate::discovery::interfaces::LocalInterface;
use crate::discovery::routing::RouteEntry;
use anyhow::Result;
use async_trait::async_trait;

/// Trait for discovery source abstraction (allows mocking in tests)
#[async_trait]
pub trait DiscoverySource: Send + Sync {
    /// Discover local interfaces
    async fn discover_interfaces(&self) -> Result<Vec<LocalInterface>>;

    /// Discover routes
    async fn discover_routes(&self) -> Result<Vec<RouteEntry>>;
}

/// Production implementation using system files
pub struct SystemDiscoverySource;

#[async_trait]
impl DiscoverySource for SystemDiscoverySource {
    async fn discover_interfaces(&self) -> Result<Vec<LocalInterface>> {
        crate::discovery::interfaces::discover_interfaces().await
    }

    async fn discover_routes(&self) -> Result<Vec<RouteEntry>> {
        crate::discovery::routing::discover_routes().await
    }
}

/// Mock implementation for testing (allows injecting failures)
#[derive(Default)]
pub struct MockDiscoverySource {
    pub should_fail_interfaces: bool,
    pub should_fail_routes: bool,
    pub interfaces: Vec<LocalInterface>,
    pub routes: Vec<RouteEntry>,
}

#[async_trait]
impl DiscoverySource for MockDiscoverySource {
    async fn discover_interfaces(&self) -> Result<Vec<LocalInterface>> {
        if self.should_fail_interfaces {
            anyhow::bail!("mock interface discovery failure");
        }
        Ok(self.interfaces.clone())
    }

    async fn discover_routes(&self) -> Result<Vec<RouteEntry>> {
        if self.should_fail_routes {
            anyhow::bail!("mock route discovery failure");
        }
        Ok(self.routes.clone())
    }
}
