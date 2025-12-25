use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;
use tokio_mpmc::Queue;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Request {
    pub id: Uuid,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Status {
    Pending,
    Ready(Vec<u8>),
    Error(String),
}

pub struct WorkerPool {
    queue: Queue<Request>,
    results: DashMap<Uuid, Status>,
    tokio: Handle,
}

impl WorkerPool {
    pub fn new(runtime_handle: Handle, worker_count: usize, queue_capacity: usize) -> Self {
        let queue = Queue::new(queue_capacity);
        let results = DashMap::new();
        let pool = WorkerPool {
            queue,
            results: results,
            tokio: runtime_handle,
        };
        pool.spawn_workers(worker_count);
        pool
    }

    fn spawn_workers(&self, worker_count: usize) {
        for _ in 0..worker_count {
            let results_clone = self.results.clone();
            let queue = self.queue.clone();
            self.tokio.spawn(async move {
                while let Some(req) = queue.receive().await.unwrap() {
                    // mark pending
                    results_clone.insert(req.id, Status::Pending);

                    // simulate async processing
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                    let result = Status::Ready(b"dummy data".to_vec());

                    results_clone.insert(req.id, result);
                }
            });
        }
    }

    pub fn make_request(&self, url: String) -> Result<Uuid, String> {
        let id = Uuid::new_v4();
        let req = Request { id, url };
        let queue = self.queue.clone();
        self.tokio
            .spawn(async move { queue.send(req).await.unwrap() });
        Ok(id)
    }

    pub fn poll(&self, request_id: Uuid) -> Option<Status> {
        self.results.get(&request_id).map(|v| v.clone())
    }
}
