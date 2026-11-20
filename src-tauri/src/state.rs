use kit_core::classifier::Classifier;
use kit_core::config::CompiledConfig;
use kit_core::db::Database;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, RwLock};

pub struct AppState {
    pub db: Arc<Mutex<Database>>,
    classifier: RwLock<Arc<Classifier>>,
    /// Cancellation flags for running background jobs, keyed by job id.
    pub jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub recovery: Mutex<Vec<String>>,
}

impl AppState {
    pub fn new(db: Arc<Mutex<Database>>, recovery: Vec<String>) -> Result<Self, Box<dyn std::error::Error>> {
        let cfg = db.lock().map_err(|_| "database lock poisoned")?.category_config()?;
        let classifier = Classifier::new(CompiledConfig::compile(&cfg)?);
        Ok(Self {
            db,
            classifier: RwLock::new(Arc::new(classifier)),
            jobs: Mutex::new(HashMap::new()),
            recovery: Mutex::new(recovery),
        })
    }

    pub fn db(&self) -> MutexGuard<'_, Database> {
        self.db.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn classifier(&self) -> Arc<Classifier> {
        self.classifier.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn set_classifier(&self, c: Classifier) {
        *self.classifier.write().unwrap_or_else(|p| p.into_inner()) = Arc::new(c);
    }

    pub fn register_job(&self, id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.jobs.lock().unwrap_or_else(|p| p.into_inner()).insert(id.to_string(), flag.clone());
        flag
    }

    pub fn finish_job(&self, id: &str) {
        self.jobs.lock().unwrap_or_else(|p| p.into_inner()).remove(id);
    }

    pub fn job_running(&self, id: &str) -> bool {
        self.jobs.lock().unwrap_or_else(|p| p.into_inner()).contains_key(id)
    }
}
