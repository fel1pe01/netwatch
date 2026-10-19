use crate::monitoring::metrics::{NETWATCH_SCHEDULER_ERRORS_TOTAL, NETWATCH_SCHEDULER_JOBS_TOTAL};
use anyhow::Result;
use futures::future::BoxFuture;
use std::sync::Arc;
use std::time::Duration;
use tokio::{task, time};

#[derive(Clone)]
pub struct Job {
    pub name: String,
    pub interval: Duration,
    pub task: Arc<dyn Fn() -> BoxFuture<'static, Result<()>> + Send + Sync>,
}

pub struct Scheduler {
    jobs: Vec<Job>,
    max_workers: usize,
    shutdown: Arc<tokio::sync::Notify>,
}

impl Scheduler {
    pub fn new(max_workers: usize) -> Self {
        Self {
            jobs: Vec::new(),
            max_workers,
            shutdown: Arc::new(tokio::sync::Notify::new()),
        }
    }

    pub fn add_job(&mut self, job: Job) {
        self.jobs.push(job);
    }

    pub async fn run(&self) -> Result<()> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Job>();
        for job in &self.jobs {
            let tx = tx.clone();
            let job = job.clone();
            let shutdown = self.shutdown.clone();
            task::spawn(async move {
                let mut ticker = time::interval(job.interval);
                loop {
                    tokio::select! {
                        _ = ticker.tick() => {
                            let _ = tx.send(job.clone());
                        }
                        _ = shutdown.notified() => {
                            break;
                        }
                    }
                }
            });
        }

        // worker pool
        let pool = Arc::new(tokio::sync::Semaphore::new(self.max_workers));
        while let Some(job) = rx.recv().await {
            let permit = pool.clone().acquire_owned().await.unwrap();
            let job_name = job.name.clone();
            let fut = (job.task)();
            task::spawn(async move {
                // Increment jobs started counter
                NETWATCH_SCHEDULER_JOBS_TOTAL
                    .with_label_values(&[&job_name])
                    .inc();

                match fut.await {
                    Ok(_) => {}
                    Err(e) => {
                        // Increment errors counter
                        NETWATCH_SCHEDULER_ERRORS_TOTAL
                            .with_label_values(&[&job_name])
                            .inc();
                        tracing::error!("Scheduler job '{}' failed: {}", job_name, e);
                    }
                }
                drop(permit);
            });
        }
        Ok(())
    }

    pub async fn shutdown(&self) {
        self.shutdown.notify_waiters();
    }
}
