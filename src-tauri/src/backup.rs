//! Complete backups contain a consistent SQLite snapshot and every registered immutable asset.
use crate::{
    package::{self, canonical, hash, keys, list, text, FILE_LIMIT},
    storage::{Store, StoreResult},
    transfer::state_hash,
    visual::{validate_asset, Asset},
};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const TABLES: &[&str] = &[
    "profiles",
    "areas",
    "modules",
    "entries",
    "blocks",
    "assets",
    "favorites",
    "search_history",
    "visit_history",
    "import_history",
    "settings",
    "synonyms",
    "personal_vocabulary",
    "schema_migrations",
];
static SERIAL: AtomicU64 = AtomicU64::new(0);
pub fn unique_id() -> String {
    format!(
        "{}-{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    )
}
#[derive(Serialize, Deserialize, Clone)]
pub struct StoredBackup {
    pub id: String,
    pub size: u64,
}
#[derive(Serialize, Deserialize)]
pub struct RestorePreview {
    pub sha256: String,
    pub review_token: String,
    pub counts: Value,
    pub current_counts: Value,
    pub files: usize,
    pub assets: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreApproval {
    pub review_token: String,
    pub phrase: String,
}
pub struct StagedAssets {
    dir: tempfile::TempDir,
    moves: Vec<(PathBuf, PathBuf)>,
    published: RefCell<Vec<PathBuf>>,
    committed: Cell<bool>,
}
impl StagedAssets {
    pub fn publish(&self) -> StoreResult<()> {
        for (source, dest) in &self.moves {
            match std::fs::hard_link(source, dest) {
                Ok(()) => self.published.borrow_mut().push(dest.clone()),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let meta = std::fs::symlink_metadata(dest)?;
                    if !meta.is_file()
                        || meta.file_type().is_symlink()
                        || meta.len() > FILE_LIMIT
                        || meta.len() != std::fs::metadata(source)?.len()
                        || hash(&std::fs::read(source)?) != hash(&std::fs::read(dest)?)
                    {
                        return Err(
                            "Conflito com arquivo gerenciado existente; conteúdo preservado".into(),
                        );
                    }
                }
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    pub fn rollback(&self) {
        for file in self.published.borrow_mut().drain(..) {
            let _ = std::fs::remove_file(file);
        }
    }
    pub fn finish(self) {
        self.committed.set(true);
    }
}
impl Drop for StagedAssets {
    fn drop(&mut self) {
        if !self.committed.get() {
            self.rollback();
        }
        let _ = self.dir.path();
    }
}
fn table_rows(conn: &Connection, table: &str) -> StoreResult<Value> {
    if !TABLES.contains(&table) {
        return Err("Tabela não permitida".into());
    }
    let mut stmt = conn.prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))?;
    let count = stmt.column_count();
    let mut query = stmt.query([])?;
    let mut rows = Vec::new();
    while let Some(row) = query.next()? {
        let mut values = Vec::new();
        for i in 0..count {
            use rusqlite::types::ValueRef;
            values.push(match row.get_ref(i)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(n) => json!(n),
                ValueRef::Real(n) => json!(n),
                ValueRef::Text(s) => json!(std::str::from_utf8(s)?),
                ValueRef::Blob(_) => return Err("Blob inesperado na base".into()),
            });
        }
        rows.push(Value::Array(values));
    }
    Ok(Value::Array(rows))
}
pub fn logical_state(conn: &Connection) -> StoreResult<String> {
    let mut data = BTreeMap::new();
    for table in TABLES {
        data.insert(*table, table_rows(conn, table)?);
    }
    Ok(hash(canonical(&serde_json::to_value(data)?).as_bytes()))
}
fn counts(conn: &Connection) -> StoreResult<Value> {
    let mut result = json!({});
    for table in TABLES {
        let n: i64 = conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
        result[*table] = json!(n);
    }
    Ok(result)
}
type SchemaRow = (String, String, String, Option<String>);
fn schema(conn: &Connection) -> StoreResult<Vec<SchemaRow>> {
    let mut stmt =
        conn.prepare("SELECT type,name,tbl_name,sql FROM sqlite_schema ORDER BY type,name")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}
fn asset_rows(conn: &Connection) -> StoreResult<Vec<Asset>> {
    let mut stmt =
        conn.prepare("SELECT id,filename,media_type,sha256,size FROM assets ORDER BY id")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Asset {
                id: r.get(0)?,
                filename: r.get(1)?,
                media_type: r.get(2)?,
                sha256: r.get(3)?,
                size: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}
struct ValidatedRestore {
    conn: Connection,
    dir: tempfile::TempDir,
    preview: RestorePreview,
    assets: Vec<(Asset, Vec<u8>)>,
}
impl Store {
    pub(crate) fn data_root(&self) -> StoreResult<PathBuf> {
        if self.path == ":memory:" {
            return Err("Backup/importação exigem base persistente".into());
        }
        let parent = Path::new(&self.path)
            .parent()
            .ok_or("Diretório de dados indisponível")?;
        Ok(parent.to_path_buf())
    }
    pub fn stage_assets(&self, assets: &[(Asset, Vec<u8>)]) -> StoreResult<StagedAssets> {
        let root = self.data_root()?.join("assets");
        std::fs::create_dir_all(&root)?;
        if std::fs::symlink_metadata(&root)?.file_type().is_symlink() {
            return Err("Diretório gerenciado não pode ser link".into());
        }
        let dir = tempfile::Builder::new()
            .prefix(".transfer-")
            .tempdir_in(&root)?;
        let mut moves = Vec::new();
        for (a, bytes) in assets {
            crate::blocks::validate_id(&a.id)?;
            validate_asset(&a.filename, &a.media_type, bytes)?;
            if hash(bytes) != a.sha256 || bytes.len() as u64 != a.size {
                return Err("Integridade de asset divergente".into());
            }
            let name = format!("{}-{}", a.id, a.sha256);
            let dest = root.join(&name);
            if dest.exists() {
                let meta = std::fs::symlink_metadata(&dest)?;
                if !meta.is_file()
                    || meta.file_type().is_symlink()
                    || meta.len() != a.size
                    || hash(&std::fs::read(&dest)?) != a.sha256
                {
                    return Err("Anexo gerenciado existente adulterado; não sobrescrito".into());
                }
                continue;
            }
            let source = dir.path().join(name);
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&source)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            moves.push((source, dest));
        }
        Ok(StagedAssets {
            dir,
            moves,
            published: RefCell::new(Vec::new()),
            committed: Cell::new(false),
        })
    }
    pub fn backup_bytes(&self) -> StoreResult<Vec<u8>> {
        let root = self.data_root()?;
        let stage = tempfile::Builder::new()
            .prefix(".backup-")
            .tempdir_in(root)?;
        let db = stage.path().join("knowledge.sqlite3");
        self.conn.backup("main", &db, None)?;
        if std::fs::metadata(&db)?.len() > FILE_LIMIT {
            return Err("Snapshot SQLite excede 50 MiB; backup abortado sem alterar a base".into());
        }
        let snapshot = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        crate::transfer::validate_database(&snapshot)?;
        let mut files = BTreeMap::new();
        files.insert("knowledge.sqlite3".into(), std::fs::read(&db)?);
        for a in asset_rows(&snapshot)? {
            files.insert(
                format!("assets/{}-{}", a.id, a.sha256),
                self.read_asset(&a.id)?,
            );
        }
        let manifest = json!({"type":"backup","format_version":1,"schema_version":4,"counts":counts(&snapshot)?,"files":files.iter().map(|(name,b)|json!({"path":name,"sha256":hash(b),"size":b.len()})).collect::<Vec<_>>()});
        files.insert(
            "backup-manifest.json".into(),
            canonical(&manifest).into_bytes(),
        );
        package::write_zip(&files)
    }
    pub fn create_backup(&self) -> StoreResult<StoredBackup> {
        let bytes = self.backup_bytes()?;
        let dir = self.data_root()?.join("backups");
        std::fs::create_dir_all(&dir)?;
        if std::fs::symlink_metadata(&dir)?.file_type().is_symlink() {
            return Err("Diretório de backups não pode ser link".into());
        }
        let id = unique_id();
        let mut f = tempfile::NamedTempFile::new_in(&dir)?;
        f.write_all(&bytes)?;
        f.as_file().sync_all()?;
        f.persist_noclobber(dir.join(format!("{id}.zip")))?;
        Ok(StoredBackup {
            id,
            size: bytes.len() as u64,
        })
    }
    pub fn list_backups(&self) -> StoreResult<Vec<StoredBackup>> {
        let dir = self.data_root()?.join("backups");
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut list = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(id) = name.strip_suffix(".zip") else {
                continue;
            };
            if crate::blocks::validate_id(id).is_err() {
                continue;
            }
            let meta = entry.metadata()?;
            if meta.is_file() && !entry.file_type()?.is_symlink() {
                list.push(StoredBackup {
                    id: id.into(),
                    size: meta.len(),
                });
            }
        }
        list.sort_by(|a, b| b.id.cmp(&a.id));
        Ok(list)
    }
    pub fn stored_backup(&self, id: &str) -> StoreResult<Vec<u8>> {
        crate::blocks::validate_id(id)?;
        let path = self.data_root()?.join("backups").join(format!("{id}.zip"));
        let meta = std::fs::symlink_metadata(&path)?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > package::TOTAL_LIMIT {
            return Err("Backup local inválido".into());
        }
        Ok(std::fs::read(path)?)
    }
    fn validate_restore(&self, bytes: &[u8]) -> StoreResult<ValidatedRestore> {
        let digest = hash(bytes);
        let mut files = package::read_zip(bytes)?;
        let manifest = package::strict_json(
            &files
                .remove("backup-manifest.json")
                .ok_or("Use um backup completo com backup-manifest.json")?,
        )?;
        keys(
            &manifest,
            &[
                "type",
                "format_version",
                "schema_version",
                "counts",
                "files",
            ],
        )?;
        if manifest["type"] != "backup"
            || manifest["format_version"] != 1
            || manifest["schema_version"] != 4
        {
            return Err("Formato/schema de backup incompatível".into());
        }
        let mut expected = HashSet::new();
        for item in list(&manifest, "files")? {
            keys(item, &["path", "sha256", "size"])?;
            let name = text(item, "path")?;
            if !expected.insert(name.to_string()) {
                return Err("Arquivo repetido no manifesto de backup".into());
            }
            let b = files.get(name).ok_or("Arquivo de backup ausente")?;
            if text(item, "sha256")? != hash(b) || item["size"].as_u64() != Some(b.len() as u64) {
                return Err("Integridade do backup divergente".into());
            }
        }
        if files.len() != expected.len() {
            return Err("Arquivo não declarado no backup".into());
        }
        let snapshot = files
            .remove("knowledge.sqlite3")
            .ok_or("Snapshot SQLite ausente")?;
        let dir = tempfile::Builder::new()
            .prefix(".restore-")
            .tempdir_in(self.data_root()?)?;
        let path = dir.path().join("snapshot.sqlite3");
        std::fs::write(&path, snapshot)?;
        let mut conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        conn.execute_batch("PRAGMA trusted_schema=OFF; PRAGMA foreign_keys=ON;")?;
        let template = Store::open(&dir.path().join("template.sqlite3"))?;
        if schema(&conn)? != schema(&template.conn)? {
            return Err("Schema SQL do backup difere do schema seguro do aplicativo".into());
        }
        drop(template);
        let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err("Snapshot SQLite corrompido".into());
        }
        let fk: Option<String> = conn
            .query_row("PRAGMA foreign_key_check", [], |r| r.get(0))
            .optional()?;
        if fk.is_some() {
            return Err("Referências inválidas no snapshot".into());
        }
        if counts(&conn)? != manifest["counts"] {
            return Err("Contagens de backup divergentes".into());
        }
        let versions = conn
            .prepare("SELECT version FROM schema_migrations ORDER BY version")?
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if versions != [1, 2, 3, 4] {
            return Err("Histórico de migrações do backup incompatível".into());
        }
        crate::transfer::validate_database(&conn)?;
        let mut assets = Vec::new();
        for a in asset_rows(&conn)? {
            crate::blocks::validate_id(&a.id)?;
            let data = files
                .remove(&format!("assets/{}-{}", a.id, a.sha256))
                .ok_or("Asset do snapshot ausente no backup")?;
            if hash(&data) != a.sha256 || data.len() as u64 != a.size {
                return Err("Asset do snapshot divergente".into());
            }
            validate_asset(&a.filename, &a.media_type, &data)?;
            assets.push((a, data));
        }
        if !files.is_empty() {
            return Err("Arquivo extra no backup".into());
        }
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM entries_fts", [])?;
        tx.execute("INSERT INTO entries_fts(entry_id,title,full_name,body) SELECT id,title,full_name,body FROM entry_search_content",[])?;
        tx.commit()?;
        let review_token = hash(format!("{digest}:{}", state_hash(&self.conn)?).as_bytes());
        let preview = RestorePreview {
            sha256: digest,
            review_token,
            counts: manifest["counts"].clone(),
            current_counts: counts(&self.conn)?,
            files: expected.len(),
            assets: assets.len(),
        };
        Ok(ValidatedRestore {
            dir,
            conn,
            preview,
            assets,
        })
    }
    pub fn preview_restore(&self, bytes: &[u8]) -> StoreResult<RestorePreview> {
        Ok(self.validate_restore(bytes)?.preview)
    }
    pub fn restore_backup(
        &mut self,
        bytes: &[u8],
        approval: RestoreApproval,
    ) -> StoreResult<StoredBackup> {
        let validated = self.validate_restore(bytes)?;
        if approval.phrase != "RESTAURAR" || approval.review_token != validated.preview.review_token
        {
            return Err("Confirmação forte inválida ou prévia desatualizada".into());
        }
        let backup = self.create_backup()?;
        let staged = self.stage_assets(&validated.assets)?;
        staged.publish()?;
        // SQLite backup owns the atomic destination transaction; source FTS has already been rebuilt.
        let result = rusqlite::backup::Backup::new(&validated.conn, &mut self.conn)?
            .run_to_completion(128, Duration::from_millis(1), None);
        if let Err(e) = result {
            return Err(e.into());
        }
        staged.finish();
        let _ = validated.dir.path();
        Ok(backup)
    }
    pub fn export_record_archive(&self, id: &str) -> StoreResult<Vec<u8>> {
        let mut manifest = self.export_record(id)?;
        let mut ids = HashSet::new();
        for b in list(&manifest["entries"][0], "blocks")? {
            for k in ["asset_id", "preview_asset_id"] {
                if let Some(id) = b[k].as_str() {
                    ids.insert(id.to_string());
                }
            }
        }
        let mut files = BTreeMap::new();
        let mut assets = Vec::new();
        let mut ids: Vec<_> = ids.into_iter().collect();
        ids.sort();
        for id in ids {
            let a = self.asset(&id)?;
            let ext = Path::new(&a.filename)
                .extension()
                .and_then(|s| s.to_str())
                .ok_or("Extensão de asset ausente")?;
            let filename = format!("{}.{}", a.id, ext);
            files.insert(format!("assets/{filename}"), self.read_asset(&id)?);
            assets.push(
                json!({"id":a.id,"filename":filename,"sha256":a.sha256,"media_type":a.media_type}),
            );
        }
        manifest["assets"] = Value::Array(assets);
        files.insert("manifest.json".into(), canonical(&manifest).into_bytes());
        package::write_zip(&files)
    }
}
use rusqlite::OptionalExtension;

impl Store {
    /// Recover only our staged hard links. Unrelated files and referenced assets are never removed.
    pub fn recover_transfer_staging(&self) -> StoreResult<()> {
        let root = self.data_root()?.join("assets");
        if !root.exists() {
            return Ok(());
        }
        if std::fs::symlink_metadata(&root)?.file_type().is_symlink() {
            return Err("Diretório de assets não pode ser link".into());
        }
        let canonical_root = root.canonicalize()?;
        for entry in std::fs::read_dir(&root)? {
            let entry = entry?;
            if !entry
                .file_name()
                .to_string_lossy()
                .starts_with(".transfer-")
                || !entry.file_type()?.is_dir()
                || entry.file_type()?.is_symlink()
            {
                continue;
            }
            let stage = entry.path();
            let resolved = stage.canonicalize()?;
            if resolved.parent() != Some(canonical_root.as_path()) {
                return Err("Staging fora do diretório gerenciado".into());
            }
            let mut files = Vec::new();
            let mut recognized = true;
            for item in std::fs::read_dir(&stage)? {
                let item = item?;
                let name = item.file_name();
                let Some(name) = name.to_str() else {
                    recognized = false;
                    break;
                };
                let Some((id, sha)) = name.rsplit_once('-') else {
                    recognized = false;
                    break;
                };
                if crate::blocks::validate_id(id).is_err()
                    || sha.len() != 64
                    || !sha
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                    || !item.file_type()?.is_file()
                    || item.file_type()?.is_symlink()
                    || item.metadata()?.len() > FILE_LIMIT
                {
                    recognized = false;
                    break;
                }
                if hash(&std::fs::read(item.path())?) != sha {
                    recognized = false;
                    break;
                }
                files.push((item.path(), root.join(name), id.to_string()));
            }
            if !recognized {
                continue;
            }
            for (source, dest, id) in files {
                let referenced:bool=self.conn.query_row("SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1) OR EXISTS(SELECT 1 FROM blocks WHERE json_extract(payload_json,'$.asset_id')=?1 OR json_extract(payload_json,'$.preview_asset_id')=?1)",[id],|r|r.get(0))?;
                if !referenced && dest.exists() && same_file::is_same_file(&source, &dest)? {
                    std::fs::remove_file(&dest)?;
                }
                std::fs::remove_file(source)?;
            }
            // Empty-only removal avoids deleting a concurrently introduced/unrecognized file.
            std::fs::remove_dir(stage)?;
        }
        Ok(())
    }
}
