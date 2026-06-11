use crate::protocol::{ManagedProcess, ProcessSnapshot};
use dashmap::DashMap;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct ProcessTable {
    pub inner: Arc<DashMap<Uuid, ManagedProcess>>,
}

impl Default for ProcessTable {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessTable {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    pub fn snapshot(&self) -> ProcessSnapshot {
        ProcessSnapshot {
            processes: self.inner.iter().map(|e| e.value().clone()).collect(),
        }
    }
}
