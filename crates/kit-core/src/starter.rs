//! Training bundled into a build ("starter" rules).
//!
//! A configuration exported from the app (Settings → Export) can be placed
//! at `starter/kit-organizer-config.json` before building. Its learned rules
//! — and, on a fresh install, its categories — are applied on first launch,
//! so someone installing that build starts with the builder's training.
//! Personal settings (output folder etc.) are never copied.
//!
//! The bundle's hash is recorded, so each bundle is merged once; a build
//! with updated training merges the new rules on its first launch.

use crate::classifier::learned::{LearnedRule, Pattern};
use crate::config::{CategoryConfig, CompiledConfig};
use crate::db::Database;
use crate::error::CoreResult;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Bundle {
    categories: Option<CategoryConfig>,
    learned: Option<Vec<LearnedRule>>,
}

#[derive(Debug, Default, PartialEq)]
pub struct StarterResult {
    pub rules: usize,
    pub categories_applied: bool,
}

/// Apply bundled training. `json` may be a full exported configuration, a
/// bare array of learned rules, or empty/`null` (nothing bundled).
pub fn apply(db: &mut Database, json: &str) -> CoreResult<Option<StarterResult>> {
    let trimmed = json.trim();
    if trimmed.is_empty() || trimmed == "null" {
        return Ok(None);
    }
    let hash = blake3::hash(trimmed.as_bytes()).to_hex().to_string();
    if db.meta("starter_bundle")?.as_deref() == Some(hash.as_str()) {
        return Ok(None);
    }
    let bundle: Bundle = match serde_json::from_str::<Vec<LearnedRule>>(trimmed) {
        Ok(rules) => Bundle { categories: None, learned: Some(rules) },
        Err(_) => serde_json::from_str(trimmed)?,
    };
    let mut res = StarterResult::default();
    // Categories only on a fresh install: never overwrite someone's own edits.
    if let Some(mut cats) = bundle.categories {
        crate::config::upgrade(&mut cats);
        if !db.has_saved_categories()? && CompiledConfig::compile(&cats).is_ok() {
            db.save_category_config(&cats)?;
            res.categories_applied = true;
        }
    }
    if let Some(rules) = bundle.learned {
        let items: Vec<(Pattern, String, String)> = rules
            .into_iter()
            .map(|r| (Pattern { kind: r.kind, tokens: r.tokens, scope: r.scope }, r.category_id, r.example))
            .collect();
        res.rules = items.len();
        db.upsert_learned_many(&items)?;
    }
    db.set_meta("starter_bundle", &hash)?;
    Ok(Some(res))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_config;

    fn bundle(extra_rule: bool) -> String {
        let mut cats = default_config();
        cats.categories.iter_mut().find(|n| n.id == "kicks").unwrap().aliases.push("thump".into());
        let mut learned = vec![serde_json::json!({
            "id": 1, "kind": "name", "tokens": ["shot"], "scope": "xlnt", "categoryId": "bass/bass-stabs",
            "example": "XLNT_Shot_01.wav", "hits": 3, "createdAt": "2026-01-01T00:00:00Z"
        })];
        if extra_rule {
            learned.push(serde_json::json!({
                "id": 2, "kind": "group", "tokens": ["pack/one_shots", "uncategorised"], "scope": null,
                "categoryId": "fx/other-fx", "example": "x.wav", "hits": 1, "createdAt": "2026-01-01T00:00:00Z"
            }));
        }
        serde_json::json!({ "app": "kit-organizer", "formatVersion": 1, "categories": cats, "learned": learned,
            "settings": { "defaultOutputDir": "C:/Users/builder/Music" } })
        .to_string()
    }

    #[test]
    fn applies_once_then_merges_updates() {
        let mut db = Database::open_in_memory().unwrap();
        let r = apply(&mut db, &bundle(false)).unwrap().unwrap();
        assert_eq!(r, StarterResult { rules: 1, categories_applied: true });
        assert_eq!(db.learned_rules().unwrap().len(), 1);
        assert!(db.category_config().unwrap().categories.iter().any(|n| n.aliases.contains(&"thump".to_string())));
        // The builder's personal settings are not copied.
        assert_eq!(db.settings().unwrap().default_output_dir, None);
        // Same bundle again: nothing happens.
        assert_eq!(apply(&mut db, &bundle(false)).unwrap(), None);
        // A newer build with more training: merged, categories left alone.
        let r = apply(&mut db, &bundle(true)).unwrap().unwrap();
        assert_eq!(r, StarterResult { rules: 2, categories_applied: false });
        assert_eq!(db.learned_rules().unwrap().len(), 2);
    }

    #[test]
    fn nothing_bundled_is_fine() {
        let mut db = Database::open_in_memory().unwrap();
        assert_eq!(apply(&mut db, "null").unwrap(), None);
        assert_eq!(apply(&mut db, "").unwrap(), None);
    }

    #[test]
    fn existing_categories_are_never_overwritten() {
        let mut db = Database::open_in_memory().unwrap();
        let mut mine = default_config();
        mine.root_folder = "My Library".into();
        db.save_category_config(&mine).unwrap();
        let r = apply(&mut db, &bundle(false)).unwrap().unwrap();
        assert!(!r.categories_applied);
        assert_eq!(db.category_config().unwrap().root_folder, "My Library");
    }
}
