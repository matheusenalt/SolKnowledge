use crate::models::*;
use crate::search::normalize;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{path::Path, time::Duration};

pub(crate) type StoreResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;
const SETTINGS: &str = include_str!("../resources/sol-settings.json");
const SEED: &str = include_str!("../resources/seed/sol-update-linux.json");
const MIGRATION: &str = include_str!("../migrations/001_core.sql");

pub struct Store {
    pub(crate) conn: Connection,
    pub(crate) path: String,
}

impl Store {
    /// Path is selected by Rust/Tauri, never by a frontend request.
    pub fn open(path: &Path) -> StoreResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )?;
        let mut store = Self {
            conn,
            path: path.display().to_string(),
        };
        store.migrate()?;
        store.seed()?;
        store.recover_transfer_staging()?;
        Ok(store)
    }

    #[cfg(test)]
    pub(crate) fn memory() -> StoreResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        let mut store = Self {
            conn,
            path: ":memory:".into(),
        };
        store.migrate()?;
        store.seed()?;
        Ok(store)
    }

    fn migrate(&mut self) -> StoreResult<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );",
        )?;
        let version: i64 = tx.query_row(
            "SELECT coalesce(max(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )?;
        if version > 4 {
            return Err("Este banco requer uma versão mais recente do SOL Knowledge".into());
        }
        if version == 0 {
            tx.execute_batch(MIGRATION)?;
            tx.execute("INSERT INTO schema_migrations(version) VALUES(1)", [])?;
        }
        if version < 2 {
            tx.execute_batch(include_str!("../migrations/002_workspace.sql"))?;
            tx.execute("INSERT INTO schema_migrations(version) VALUES(2)", [])?;
        }
        if version < 3 {
            tx.execute_batch(include_str!("../migrations/003_visual.sql"))?;
            tx.execute("INSERT INTO schema_migrations(version) VALUES(3)", [])?;
        }
        if version < 4 {
            tx.execute_batch(include_str!("../migrations/004_transfer.sql"))?;
            let seed: Value = serde_json::from_str(SEED)?;
            for (id, content) in [
                ("builtin:settings:v2", SETTINGS),
                (text(&seed, "package_id")?, SEED),
            ] {
                tx.execute("UPDATE import_history SET sha256=?1,hash_format='canonical-v1' WHERE package_id=?2 AND sha256=?3",params![digest(content),id,crate::package::hash(content.as_bytes())])?;
            }
            tx.execute("INSERT INTO schema_migrations(version) VALUES(4)", [])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn seed(&mut self) -> StoreResult<()> {
        let settings: Value = serde_json::from_str(SETTINGS)?;
        let seed: Value = serde_json::from_str(SEED)?;
        if seed["schema_version"] != 1 || seed["type"] != "knowledge_update" {
            return Err("Seed Linux incompatível".into());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !package_applied(&tx, "builtin:settings:v2", SETTINGS)? {
            for profile in array(&settings, "profiles")? {
                tx.execute(
                    "INSERT OR IGNORE INTO profiles(id,name,payload_json) VALUES(?1,?2,?3)",
                    params![
                        text(profile, "id")?,
                        text(profile, "name")?,
                        profile.to_string()
                    ],
                )?;
            }
            seed_hierarchy(&tx, &settings)?;
            for key in ["app", "appearance", "preferences"] {
                tx.execute(
                    "INSERT OR IGNORE INTO settings(key,value_json) VALUES(?1,?2)",
                    params![key, settings[key].to_string()],
                )?;
            }
            record_package(&tx, "builtin:settings:v2", SETTINGS, "settings")?;
        }
        let package = text(&seed, "package_id")?;
        if !package_applied(&tx, package, SEED)? {
            seed_hierarchy(&tx, &seed)?;
            for entry in array(&seed, "entries")? {
                // Do not replace existing personal data, including edited seed entries.
                tx.execute(
                    "INSERT OR IGNORE INTO entries(id,module_id,payload_json) VALUES(?1,?2,?3)",
                    params![
                        text(entry, "id")?,
                        text(entry, "module_id")?,
                        entry.to_string()
                    ],
                )?;
            }
            // Reference synonyms only. Personal vocabulary always starts empty.
            for (phrase, target) in [
                ("ficheiros ocultos", "linux-cmd-ls"),
                ("espaço livre", "linux-cmd-df"),
                ("gerenciar serviços", "linux-cmd-systemctl"),
            ] {
                tx.execute("INSERT OR IGNORE INTO synonyms(phrase,normalized,target_entry_id) VALUES(?1,?2,?3)",
                    params![phrase, normalize(phrase), target])?;
            }
            record_package(&tx, package, SEED, "seed")?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn overview(&self) -> StoreResult<Overview> {
        Ok(Overview {
            entries: self
                .conn
                .query_row("SELECT count(*) FROM entries", [], |r| r.get(0))?,
            options: self.conn.query_row(
                "SELECT coalesce(sum(json_array_length(payload_json,'$.options')),0) FROM entries",
                [],
                |r| r.get(0),
            )?,
            linux_modules: self.conn.query_row(
                "SELECT count(*) FROM modules WHERE area_id='linux'",
                [],
                |r| r.get(0),
            )?,
            areas: self
                .conn
                .query_row("SELECT count(*) FROM areas", [], |r| r.get(0))?,
            profiles: self
                .conn
                .query_row("SELECT count(*) FROM profiles", [], |r| r.get(0))?,
            schema_version: self.conn.query_row(
                "SELECT max(version) FROM schema_migrations",
                [],
                |r| r.get(0),
            )?,
            database_path: self.path.clone(),
        })
    }

    pub fn entries_index(&self) -> StoreResult<Vec<EntryIndex>> {
        let mut stmt = self.conn.prepare("SELECT e.id,e.title,m.name FROM entries e JOIN modules m ON m.id=e.module_id ORDER BY e.title,e.id")?;
        let rows = stmt.query_map([], |r| {
            Ok(EntryIndex {
                id: r.get(0)?,
                title: r.get(1)?,
                module_name: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn entry(&self, id: &str) -> StoreResult<EntryDetail> {
        let (payload, area_name, module_name, area_id, module_id, revision, favorite): (String,String,String,String,String,i64,bool) = self.conn.query_row(
            "SELECT e.payload_json,a.name,m.name,a.id,m.id,e.revision,EXISTS(SELECT 1 FROM favorites WHERE entry_id=e.id) FROM entries e JOIN modules m ON m.id=e.module_id JOIN areas a ON a.id=m.area_id WHERE e.id=?1",
            [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?)))?;
        let payload: Value = serde_json::from_str(&payload)?;
        let mut views = crate::blocks::legacy_blocks(&payload);
        let mut blocks = self
            .conn
            .prepare("SELECT payload_json FROM blocks WHERE entry_id=?1 ORDER BY position,id")?;
        let blocks: Vec<String> = blocks
            .query_map([id], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        for block in blocks {
            views.push(BlockView {
                payload: serde_json::from_str(&block)?,
                virtual_block: false,
            });
        }
        let presentation = self.resolve_presentation(&module_id)?;
        Ok(EntryDetail {
            id: id.into(),
            area_name,
            module_name,
            payload,
            area_id,
            module_id,
            revision,
            favorite,
            presentation,
            blocks: views,
        })
    }

    pub fn vocabulary(&self) -> StoreResult<Vec<VocabularyTerm>> {
        let mut terms = Vec::new();
        for kind in [TermKind::Synonym, TermKind::Personal] {
            // Table name comes only from the Rust enum, never from user SQL.
            let mut stmt = self.conn.prepare(&format!("SELECT v.id,v.phrase,v.target_entry_id,e.title FROM {} v JOIN entries e ON e.id=v.target_entry_id ORDER BY v.phrase,v.id",kind.table()))?;
            terms.extend(
                stmt.query_map([], |r| {
                    Ok(VocabularyTerm {
                        id: r.get(0)?,
                        kind,
                        phrase: r.get(1)?,
                        target_entry_id: r.get(2)?,
                        target_title: r.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?,
            );
        }
        Ok(terms)
    }

    pub fn save_term(&mut self, input: TermInput) -> StoreResult<i64> {
        let phrase = input.phrase.trim();
        let normalized = normalize(phrase);
        if phrase.chars().count() > 128
            || !normalized.chars().any(char::is_alphanumeric)
            || phrase.chars().any(char::is_control)
        {
            return Err("Use uma expressão de 1 a 128 caracteres, sem controles".into());
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let table = input.kind.table();
        let id = if let Some(id) = input.id {
            let changed = tx.execute(
                &format!(
                    "UPDATE {table} SET phrase=?1,normalized=?2,target_entry_id=?3 WHERE id=?4"
                ),
                params![phrase, normalized, input.target_entry_id, id],
            )?;
            if changed != 1 {
                return Err("Termo não encontrado; recarregue o vocabulário".into());
            }
            id
        } else {
            tx.execute(
                &format!("INSERT INTO {table}(phrase,normalized,target_entry_id) VALUES(?1,?2,?3)"),
                params![phrase, normalized, input.target_entry_id],
            )?;
            tx.last_insert_rowid()
        };
        tx.commit()?;
        Ok(id)
    }

    pub fn delete_term(&mut self, kind: TermKind, id: i64) -> StoreResult<()> {
        let changed = self
            .conn
            .execute(&format!("DELETE FROM {} WHERE id=?1", kind.table()), [id])?;
        if changed != 1 {
            return Err("Termo não encontrado".into());
        }
        Ok(())
    }
}

fn digest(content: &str) -> String {
    let canonical = serde_json::from_str::<Value>(content)
        .map(|v| crate::package::canonical(&v))
        .unwrap_or_else(|_| content.to_string());
    Sha256::digest(canonical.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn package_applied(tx: &Transaction<'_>, id: &str, content: &str) -> StoreResult<bool> {
    let previous: Option<String> = tx
        .query_row(
            "SELECT sha256 FROM import_history WHERE package_id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()?;
    match previous {
        Some(hash) if hash == digest(content) => Ok(true),
        Some(_) => Err("O pacote de referência mudou sem receber novo ID; banco preservado".into()),
        None => Ok(false),
    }
}

fn record_package(tx: &Transaction<'_>, id: &str, content: &str, kind: &str) -> StoreResult<()> {
    tx.execute(
        "INSERT INTO import_history(package_id,sha256,kind,hash_format) VALUES(?1,?2,?3,'canonical-v1')",
        params![id, digest(content), kind],
    )?;
    Ok(())
}

fn array<'a>(value: &'a Value, key: &str) -> StoreResult<&'a Vec<Value>> {
    value[key]
        .as_array()
        .ok_or_else(|| format!("Array obrigatório ausente: {key}").into())
}
fn text<'a>(value: &'a Value, key: &str) -> StoreResult<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| format!("Texto obrigatório ausente: {key}").into())
}
fn seed_hierarchy(tx: &Transaction<'_>, source: &Value) -> StoreResult<()> {
    for area in array(source, "areas")? {
        tx.execute(
            "INSERT OR IGNORE INTO areas(id,name,profile_id,payload_json) VALUES(?1,?2,?3,?4)",
            params![
                text(area, "id")?,
                text(area, "name")?,
                area["profile_id"].as_str(),
                area.to_string()
            ],
        )?;
    }
    for module in array(source, "modules")? {
        tx.execute("INSERT OR IGNORE INTO modules(id,area_id,parent_id,name,profile_id,payload_json) VALUES(?1,?2,?3,?4,?5,?6)",
            params![text(module,"id")?,text(module,"area_id")?,module["parent_id"].as_str(),text(module,"name")?,module["profile_id"].as_str(),module.to_string()])?;
    }
    Ok(())
}
