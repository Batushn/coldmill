//! Worker pool and cancellation registry.
//!
//! Concurrency is capped at the core count: each job is its own ffmpeg process,
//! and ffmpeg already threads internally, so oversubscribing only thrashes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tauri_plugin_shell::process::CommandChild;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

struct JobEntry {
    /// `None` while the job is still waiting for a permit, or once it ended.
    child: Option<CommandChild>,
    cancelled: bool,
}

pub struct JobRegistry {
    permits: Arc<Semaphore>,
    concurrency: usize,
    jobs: Mutex<HashMap<String, JobEntry>>,
}

impl JobRegistry {
    pub fn new() -> Self {
        let concurrency = num_cpus::get().max(1);
        Self {
            permits: Arc::new(Semaphore::new(concurrency)),
            concurrency,
            jobs: Mutex::new(HashMap::new()),
        }
    }

    pub fn concurrency(&self) -> usize {
        self.concurrency
    }

    /// Called before the worker task is spawned so a cancel arriving while the
    /// job is still queued is not lost.
    pub fn register(&self, job_id: &str) {
        self.jobs.lock().unwrap().insert(
            job_id.to_string(),
            JobEntry {
                child: None,
                cancelled: false,
            },
        );
    }

    pub async fn acquire(&self) -> OwnedSemaphorePermit {
        // The semaphore is never closed, so this cannot fail.
        Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .expect("semaphore closed")
    }

    pub fn is_cancelled(&self, job_id: &str) -> bool {
        self.jobs
            .lock()
            .unwrap()
            .get(job_id)
            .map(|e| e.cancelled)
            .unwrap_or(true)
    }

    /// Hands the running process to the registry so `cancel` can reach it.
    /// Returns the child back when the job was cancelled while spawning — the
    /// caller must then kill it.
    #[must_use]
    pub fn attach_child(&self, job_id: &str, child: CommandChild) -> Option<CommandChild> {
        let mut jobs = self.jobs.lock().unwrap();
        match jobs.get_mut(job_id) {
            Some(entry) if !entry.cancelled => {
                entry.child = Some(child);
                None
            }
            _ => Some(child),
        }
    }

    pub fn finish(&self, job_id: &str) {
        self.jobs.lock().unwrap().remove(job_id);
    }

    /// Kills the process if it is running. Returns false for unknown ids.
    pub fn cancel(&self, job_id: &str) -> bool {
        // Take the child out under the lock, kill it outside: `kill` consumes
        // the handle and we do not want to hold the mutex across it.
        let child = {
            let mut jobs = self.jobs.lock().unwrap();
            match jobs.get_mut(job_id) {
                Some(entry) => {
                    entry.cancelled = true;
                    entry.child.take()
                }
                None => return false,
            }
        };
        if let Some(child) = child {
            let _ = child.kill();
        }
        true
    }

    /// Cancels everything in flight, returning the ids that were affected.
    pub fn cancel_all(&self) -> Vec<String> {
        let mut victims = Vec::new();
        let children = {
            let mut jobs = self.jobs.lock().unwrap();
            let mut children = Vec::new();
            for (id, entry) in jobs.iter_mut() {
                entry.cancelled = true;
                victims.push(id.clone());
                if let Some(child) = entry.child.take() {
                    children.push(child);
                }
            }
            children
        };
        for child in children {
            let _ = child.kill();
        }
        victims
    }
}

impl Default for JobRegistry {
    fn default() -> Self {
        Self::new()
    }
}
