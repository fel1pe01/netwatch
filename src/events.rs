use anyhow::Result;
use tokio::sync::broadcast::{channel, Receiver, Sender};

#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("broadcast error: {0}")]
    Broadcast(#[from] tokio::sync::broadcast::error::SendError<String>),
}

pub struct EventBus {
    sender: Sender<String>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _receiver) = channel(1000);
        Self { sender }
    }

    pub fn subscribe(&self) -> Receiver<String> {
        self.sender.subscribe()
    }

    pub async fn emit(&self, msg: String) -> Result<(), EventError> {
        let _ = self.sender.send(msg).map(|_| ());
        Ok(())
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
