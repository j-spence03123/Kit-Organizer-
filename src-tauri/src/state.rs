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
        let classifier = build_classifier(&*db.lock().map_err(|_| "database lock poisoned")?)?;
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

    /// Rebuild the classifier from the saved configuration and learned rules.
    pub fn rebuild_classifier(&self) -> kit_core::CoreResult<()> {
        let c = build_classifier(&self.db())?;
        self.set_classifier(c);
        Ok(())
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

/// Classifier with the saved categories plus rules learned from corrections.
pub fn build_classifier(db: &Database) -> kit_core::CoreResult<Classifier> {
    let cfg = db.category_config()?;
    Ok(Classifier::new(CompiledConfig::compile(&cfg)?).with_learned(db.learned_rules()?))
}
