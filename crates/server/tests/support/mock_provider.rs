use std::collections::VecDeque;
use std::sync::Mutex;

use async_trait::async_trait;
use composer_server::provider::{ModelEvent, ModelRequest, ModelStream, Provider};
use tokio::sync::mpsc;

/// A provider that plays back scripted responses, one per model call, and records requests.
#[derive(Default)]
pub struct MockProvider {
    scripts: Mutex<VecDeque<Vec<ModelEvent>>>,
    pub requests: Mutex<Vec<ModelRequest>>,
}

impl MockProvider {
    pub fn new(scripts: Vec<Vec<ModelEvent>>) -> Self {
        Self {
            scripts: Mutex::new(scripts.into()),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn requests(&self) -> Vec<ModelRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn model(&self) -> &str {
        "mock"
    }

    async fn stream(&self, request: ModelRequest) -> Result<ModelStream, String> {
        self.requests.lock().unwrap().push(request);
        let events = self
            .scripts
            .lock()
            .unwrap()
            .pop_front()
            .ok_or("mock: no script left")?;
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            for event in events {
                if tx.send(Ok(event)).await.is_err() {
                    return;
                }
            }
        });
        Ok(rx)
    }
}
