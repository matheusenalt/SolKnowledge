use crate::{
    backup::RestoreApproval,
    package::{self, canonical, hash},
    storage::{Store, StoreResult},
    transfer::{state_hash, ImportApproval, ImportInput},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
};
fn input(value: Value) -> ImportInput {
    ImportInput {
        format: "json".into(),
        mode: value["type"].as_str().unwrap().into(),
        bytes: serde_json::to_vec(&value).unwrap(),
    }
}
fn approve(db: &mut Store, p: &ImportInput) -> StoreResult<crate::transfer::Applied> {
    let preview = db.preview_package(p)?;
    let approval = ImportApproval {
        review_token: preview.review_token,
        approved_changes: preview
            .changes
            .into_iter()
            .filter(|c| c.requires_approval)
            .map(|c| c.id)
            .collect(),
        confirm: true,
    };
    db.apply_package(p, approval)
}
fn fixture() -> Value {
    serde_json::from_str(include_str!("../../examples/topologia.json")).unwrap()
}
fn new_record() -> Value {
    json!({"schema_version":2,"type":"knowledge_update","package_id":"merge-create","entries":[{"id":"merge-fixture","module_id":"redes-topologias","type":"concept","title":"Inicial","personal_description":"Nota pessoal fictícia de teste","keywords":["original"],"sources":["fonte original"],"related_entries":["ainda-a-importar"],"learning_status":"studying","options":[{"id":"one","example":"somente referência","description":"descrição original","featured":true}],"blocks":[{"id":"note","type":"personal_note","content":"Nota de bloco fictícia"},{"id":"list","type":"cabling","items":["porta A","porta B"]}]}]})
}
#[test]
fn partial_merge_omissions_options_blocks_confirmations_and_canonical_idempotency(
) -> StoreResult<()> {
    let root = tempfile::tempdir()?;
    let mut db = Store::open(&root.path().join("knowledge.sqlite3"))?;
    approve(&mut db, &input(new_record()))?;
    let partial = input(
        json!({"schema_version":2,"type":"knowledge_update","package_id":"merge-partial","entries":[{"id":"merge-fixture","title":"Atualizado","keywords":["novo","original"],"sources":["nova fonte"],"related_entries":["outra-referencia"],"options":[{"id":"one","application":"novo contexto"},{"id":"two","example":"outra referência"}],"blocks":[{"id":"list","items":["porta C"]}]}]}),
    );
    let before = state_hash(&db.conn)?;
    let preview = db.preview_package(&partial)?;
    assert_eq!(state_hash(&db.conn)?, before);
    assert!(preview.changes.iter().any(|c| c.id.ends_with("/title")
        && c.before == "Inicial"
        && c.after == "Atualizado"
        && c.requires_approval));
    assert!(db
        .apply_package(
            &partial,
            ImportApproval {
                review_token: preview.review_token.clone(),
                approved_changes: vec![],
                confirm: true
            }
        )
        .is_err());
    assert_eq!(state_hash(&db.conn)?, before);
    let result = approve(&mut db, &partial)?;
    assert!(result.backup_id.is_some());
    let e = db.entry("merge-fixture")?;
    assert_eq!(
        e.payload["personal_description"],
        "Nota pessoal fictícia de teste"
    );
    assert_eq!(e.payload["learning_status"], "studying");
    assert_eq!(e.payload["keywords"], json!(["original", "novo"]));
    assert_eq!(e.payload["options"][0]["description"], "descrição original");
    assert_eq!(e.payload["options"][0]["application"], "novo contexto");
    assert_eq!(e.payload["options"].as_array().unwrap().len(), 2);
    assert_eq!(
        e.blocks
            .iter()
            .find(|b| b.payload["id"] == "list")
            .unwrap()
            .payload["items"],
        json!(["porta C"])
    );
    assert_eq!(
        e.blocks
            .iter()
            .find(|b| b.payload["id"] == "note")
            .unwrap()
            .payload["content"],
        "Nota de bloco fictícia"
    );
    let backups = db.list_backups()?.len();
    let mut canonical_input = ImportInput {
        bytes: canonical(&package::strict_json(&partial.bytes)?).into_bytes(),
        ..partial
    };
    assert!(approve(&mut db, &canonical_input)?.preview.already_applied);
    assert_eq!(backups, db.list_backups()?.len());
    let mut changed: Value = package::strict_json(&canonical_input.bytes)?;
    changed["entries"][0]["title"] = json!("Diferente");
    canonical_input.bytes = serde_json::to_vec(&changed)?;
    assert!(db.preview_package(&canonical_input).is_err());
    let personal = input(
        json!({"schema_version":2,"type":"knowledge_update","package_id":"personal-patch","entries":[{"id":"merge-fixture","personal_description":"","learning_status":"applied","blocks":[{"id":"note","content":""}]}]}),
    );
    let p = db.preview_package(&personal)?;
    assert!(p.changes.iter().filter(|c| c.requires_approval).count() >= 3);
    approve(&mut db, &personal)?;
    let e = db.entry("merge-fixture")?;
    assert_eq!(e.payload["personal_description"], "");
    assert_eq!(e.payload["title"], "Atualizado");
    assert_eq!(e.payload["learning_status"], "applied");
    Ok(())
}
#[test]
fn stale_or_cancelled_preview_and_invalid_packages_never_write() -> StoreResult<()> {
    let root = tempfile::tempdir()?;
    let mut db = Store::open(&root.path().join("knowledge.sqlite3"))?;
    let p = input(fixture());
    let preview = db.preview_package(&p)?;
    let before = state_hash(&db.conn)?;
    assert!(db
        .apply_package(
            &p,
            ImportApproval {
                review_token: preview.review_token.clone(),
                approved_changes: vec![],
                confirm: false
            }
        )
        .is_err());
    assert_eq!(state_hash(&db.conn)?, before);
    assert!(db.list_backups()?.is_empty());
    db.conn.execute(
        "INSERT INTO search_history(query) VALUES('concurrent edit')",
        [],
    )?;
    assert!(db
        .apply_package(
            &p,
            ImportApproval {
                review_token: preview.review_token,
                approved_changes: vec![],
                confirm: true
            }
        )
        .is_err());
    assert!(db.entry("redes-estrela-demo").is_err());
    let mut invalids = Vec::new();
    let mut v = fixture();
    v["entries"][0]["blocks"][0]["edges"][0]["to"] = json!("absent");
    invalids.push(v);
    let mut v = fixture();
    v["entries"][0]["blocks"][0]["type"] = json!("unknown");
    invalids.push(v);
    let mut v = fixture();
    v["entries"][0]["module_id"] = json!("missing");
    invalids.push(v);
    let mut v = fixture();
    v["extra"] = json!(true);
    invalids.push(v);
    let mut v = fixture();
    v["entries"][0]["title"] = Value::Null;
    invalids.push(v);
    let mut v = fixture();
    let duplicate = v["entries"][0].clone();
    v["entries"].as_array_mut().unwrap().push(duplicate);
    invalids.push(v);
    for v in invalids {
        let before = state_hash(&db.conn)?;
        assert!(db.preview_package(&input(v)).is_err());
        assert_eq!(state_hash(&db.conn)?, before);
    }
    for bytes in [br#"{"schema_version":2,"type":"knowledge_update","package_id":"dup","entries":[],"entries":[]}"#.as_slice(),b"{/* no comments */}",b"{} trailing"]{assert!(package::strict_json(bytes).is_err());}
    assert!(db.list_backups()?.is_empty());
    Ok(())
}
#[test]
fn settings_hierarchy_cycles_reversal_profiles_and_v1_are_validated() -> StoreResult<()> {
    let root = tempfile::tempdir()?;
    let mut db = Store::open(&root.path().join("knowledge.sqlite3"))?;
    let settings = input(
        json!({"schema_version":2,"type":"settings","profiles":[{"id":"fixture-profile","name":"Fixture","enabled_components":["topology","diagram"]}],"areas":[{"id":"fixture-area","name":"Fixture","profile_id":"fixture-profile"}],"modules":[{"id":"fixture-child","area_id":"fixture-area","parent_id":"fixture-parent","name":"Filho"},{"id":"fixture-parent","area_id":"fixture-area","name":"Pai"}]}),
    );
    approve(&mut db, &settings)?;
    let before = state_hash(&db.conn)?;
    let bad = input(
        json!({"schema_version":2,"type":"settings","modules":[{"id":"fixture-parent","parent_id":"fixture-child"}]}),
    );
    assert!(db.preview_package(&bad).is_err());
    assert_eq!(state_hash(&db.conn)?, before);
    let reverse = input(
        json!({"schema_version":2,"type":"settings","modules":[{"id":"fixture-parent","parent_id":"fixture-child"},{"id":"fixture-child","parent_id":null}],"appearance":{"primary_color":"#FFCC00"}}),
    );
    approve(&mut db, &reverse)?;
    assert_eq!(
        db.export_settings()?["appearance"]["primary_color"],
        "#FFCC00"
    );
    assert_eq!(
        db.export_settings()?["appearance"]["background_color"],
        "#101010"
    );
    let legacy = input(
        json!({"schema_version":1,"type":"knowledge_update","package_id":"legacy-new","entries":[{"id":"legacy-fixture","module_id":"fixture-parent","type":"concept","title":"Legado"}]}),
    );
    approve(&mut db, &legacy)?;
    let e = db.entry("legacy-fixture")?;
    assert_eq!(e.payload["learning_status"], "not_assessed");
    let exported = db.export_settings()?;
    assert_eq!(db.preview_package(&input(exported))?.counts.updated, 0);
    let invalid = input(
        json!({"schema_version":2,"type":"settings","profiles":[{"id":"fixture-profile","enabled_components":["unknown"]}]}),
    );
    assert!(db.preview_package(&invalid).is_err());
    Ok(())
}
fn multimedia() -> StoreResult<ImportInput> {
    let mut v = fixture();
    v["package_id"] = json!("zip-fixture");
    let png = include_bytes!("../icons/sol.png").to_vec();
    let drawio = b"<mxfile><diagram>original opaque fixture</diagram></mxfile>".to_vec();
    v["assets"] = json!([{"id":"png-fixture","filename":"preview.png","sha256":hash(&png),"media_type":"image/png"},{"id":"drawio-fixture","filename":"original.drawio","sha256":hash(&drawio),"media_type":"application/x-drawio"}]);
    let a = v["entries"][0]["blocks"].as_array_mut().unwrap();
    a.push(json!({"id":"original","type":"attachment","asset_id":"drawio-fixture","preview_asset_id":"png-fixture"}));
    let files = BTreeMap::from([
        ("manifest.json".into(), serde_json::to_vec(&v)?),
        ("assets/preview.png".into(), png),
        ("assets/original.drawio".into(), drawio),
    ]);
    Ok(ImportInput {
        format: "zip".into(),
        mode: "knowledge_update".into(),
        bytes: package::write_zip(&files)?,
    })
}
#[test]
fn zip_with_png_and_drawio_full_backup_restore_clean_database_and_search() -> StoreResult<()> {
    let root = tempfile::tempdir()?;
    let mut db = Store::open(&root.path().join("source/knowledge.sqlite3"))?;
    let p = multimedia()?;
    let before = state_hash(&db.conn)?;
    db.preview_package(&p)?;
    assert_eq!(state_hash(&db.conn)?, before);
    assert!(!root.path().join("source/assets/png-fixture").exists());
    approve(&mut db, &p)?;
    assert_eq!(
        db.read_asset("png-fixture")?,
        include_bytes!("../icons/sol.png")
    );
    db.favorite("redes-estrela-demo", true)?;
    db.visit_entry("redes-estrela-demo")?;
    let export = db.export_record_archive("redes-estrela-demo")?;
    let parsed = package::Package::read(&export, "zip", "knowledge_update")?;
    assert_eq!(parsed.assets.len(), 2);
    let backup = db.backup_bytes()?;
    let files = package::read_zip(&backup)?;
    assert!(files.contains_key("backup-manifest.json"));
    assert_eq!(files.len(), 4);
    let mut clean = Store::open(&root.path().join("clean/knowledge.sqlite3"))?;
    assert!(clean.entry("redes-estrela-demo").is_err());
    let before = state_hash(&clean.conn)?;
    let preview = clean.preview_restore(&backup)?;
    assert_eq!(state_hash(&clean.conn)?, before);
    assert_eq!(preview.assets, 2);
    assert!(clean
        .restore_backup(
            &backup,
            RestoreApproval {
                review_token: preview.review_token.clone(),
                phrase: "wrong".into()
            }
        )
        .is_err());
    assert_eq!(state_hash(&clean.conn)?, before);
    let previous = clean.restore_backup(
        &backup,
        RestoreApproval {
            review_token: preview.review_token,
            phrase: "RESTAURAR".into(),
        },
    )?;
    assert!(clean.stored_backup(&previous.id)?.len() > 100);
    assert_eq!(state_hash(&clean.conn)?, state_hash(&db.conn)?);
    assert_eq!(
        clean.read_asset("png-fixture")?,
        db.read_asset("png-fixture")?
    );
    assert_eq!(
        clean.read_asset("drawio-fixture")?,
        db.read_asset("drawio-fixture")?
    );
    assert!(clean.entry("redes-estrela-demo")?.favorite);
    for query in [
        "Ethernet UTP",
        "switch",
        "ligação de computadores",
        "original drawio",
    ] {
        assert!(
            clean
                .search(crate::models::SearchRequest {
                    query: query.into(),
                    limit: None,
                    filters: None
                })?
                .iter()
                .any(|e| e.id == "redes-estrela-demo"),
            "{query}"
        );
    }
    let missing_backup = clean.stored_backup(&previous.id)?;
    let preview = clean.preview_restore(&missing_backup)?;
    clean.restore_backup(
        &missing_backup,
        RestoreApproval {
            review_token: preview.review_token,
            phrase: "RESTAURAR".into(),
        },
    )?;
    assert!(clean.entry("redes-estrela-demo").is_err());
    assert!(!std::fs::read_dir(root.path().join("clean"))?.any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".restore-")));
    Ok(())
}
fn raw_zip(members: &[(&str, &[u8], Option<u32>)]) -> StoreResult<Vec<u8>> {
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes, mode) in members {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(mode.unwrap_or(0o600));
        if mode == &Some(0o120777) {
            z.add_symlink(*name, "outside", options)?;
        } else {
            z.start_file(*name, options)?;
            z.write_all(bytes)?;
        }
    }
    Ok(z.finish()?.into_inner())
}
#[test]
fn archive_traversal_links_duplicates_hash_mismatch_and_extra_files_rejected() -> StoreResult<()> {
    let root = tempfile::tempdir()?;
    let db = Store::open(&root.path().join("knowledge.sqlite3"))?;
    let good = multimedia()?;
    let valid = package::read_zip(&good.bytes)?;
    let manifest = &valid["manifest.json"];
    for name in [
        "../escape.png",
        "/absolute",
        "C:/absolute",
        "assets/../escape",
        "assets/nested/file",
        "assets\\evil",
        "assets/colon:stream",
    ] {
        let bytes = raw_zip(&[("manifest.json", manifest, None), (name, b"payload", None)])?;
        let p = ImportInput {
            bytes,
            ..multimedia()?
        };
        assert!(db.preview_package(&p).is_err(), "{name}");
    }
    let duplicate = raw_zip(&[
        ("manifest.json", manifest, None),
        ("MANIFEST.JSON", manifest, None),
    ])?;
    assert!(package::read_zip(&duplicate).is_err());
    let symlink = raw_zip(&[
        ("manifest.json", manifest, None),
        ("assets/link", b"", Some(0o120777)),
    ])?;
    assert!(package::read_zip(&symlink).is_err());
    let mut bad = valid.clone();
    bad.insert("assets/preview.png".into(), b"wrong".to_vec());
    assert!(db
        .preview_package(&ImportInput {
            bytes: package::write_zip(&bad)?,
            ..multimedia()?
        })
        .is_err());
    let mut bad = valid;
    bad.insert("assets/undeclared.png".into(), b"anything".to_vec());
    assert!(db
        .preview_package(&ImportInput {
            bytes: package::write_zip(&bad)?,
            ..multimedia()?
        })
        .is_err());
    assert!(db.list_backups()?.is_empty());
    assert!(db.entry("redes-estrela-demo").is_err());
    assert_eq!(
        std::fs::read_dir(root.path())?
            .filter(|e| e
                .as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".transfer-"))
            .count(),
        0
    );
    Ok(())
}
#[test]
fn stage_failure_rolls_back_sql_fts_history_and_unreferenced_files() -> StoreResult<()> {
    let root = tempfile::tempdir()?;
    let mut db = Store::open(&root.path().join("knowledge.sqlite3"))?;
    let p = multimedia()?;
    let preview = db.preview_package(&p)?;
    let prepared = package::Package::read(&p.bytes, "zip", "knowledge_update")?;
    {
        let staged = db.stage_assets(&prepared.assets)?;
        staged.publish()?;
        assert!(root
            .path()
            .join(format!(
                "assets/png-fixture-{}",
                prepared
                    .assets
                    .iter()
                    .find(|(a, _)| a.id == "png-fixture")
                    .unwrap()
                    .0
                    .sha256
            ))
            .exists());
    }
    assert_eq!(std::fs::read_dir(root.path().join("assets"))?.count(), 0);
    // A conflicting final file after preview must abort without overwriting it or changing SQLite.
    let a = &prepared.assets[0].0;
    let conflict = root.path().join(format!("assets/{}-{}", a.id, a.sha256));
    std::fs::write(&conflict, b"preserve this foreign file")?;
    let before = state_hash(&db.conn)?;
    assert!(db
        .apply_package(
            &p,
            ImportApproval {
                review_token: preview.review_token,
                approved_changes: vec![],
                confirm: true
            }
        )
        .is_err());
    assert_eq!(state_hash(&db.conn)?, before);
    assert_eq!(std::fs::read(conflict)?, b"preserve this foreign file");
    assert!(db.entry("redes-estrela-demo").is_err());
    assert_eq!(
        db.conn.query_row(
            "SELECT count(*) FROM entries_fts WHERE entry_id='redes-estrela-demo'",
            [],
            |r| r.get::<_, i64>(0)
        )?,
        0
    );
    assert_eq!(std::fs::read_dir(root.path().join("assets"))?.count(), 1);
    Ok(())
}
#[test]
fn tampered_backup_schema_counts_files_and_sqlite_never_replace_current_data() -> StoreResult<()> {
    let root = tempfile::tempdir()?;
    let mut db = Store::open(&root.path().join("knowledge.sqlite3"))?;
    approve(&mut db, &input(fixture()))?;
    let bytes = db.backup_bytes()?;
    let files = package::read_zip(&bytes)?;
    let before = state_hash(&db.conn)?;
    for mode in ["hash", "count", "extra", "schema"] {
        let mut files = files.clone();
        let mut manifest: Value = package::strict_json(&files["backup-manifest.json"])?;
        match mode {
            "hash" => {
                files.get_mut("knowledge.sqlite3").unwrap()[100] ^= 1;
            }
            "count" => manifest["counts"]["entries"] = json!(999),
            "extra" => {
                files.insert("assets/extra".into(), b"extra".to_vec());
            }
            _ => {
                let path = root.path().join("malicious.sqlite3");
                std::fs::write(&path, &files["knowledge.sqlite3"])?;
                let conn = rusqlite::Connection::open(&path)?;
                conn.execute_batch("CREATE TRIGGER injected AFTER INSERT ON favorites BEGIN DELETE FROM entries; END;")?;
                drop(conn);
                let data = std::fs::read(path)?;
                files.insert("knowledge.sqlite3".into(), data.clone());
                let dbfile = manifest["files"]
                    .as_array_mut()
                    .unwrap()
                    .iter_mut()
                    .find(|f| f["path"] == "knowledge.sqlite3")
                    .unwrap();
                dbfile["sha256"] = json!(hash(&data));
                dbfile["size"] = json!(data.len());
            }
        }
        files.insert(
            "backup-manifest.json".into(),
            serde_json::to_vec(&manifest)?,
        );
        assert!(
            db.preview_restore(&package::write_zip(&files)?).is_err(),
            "{mode}"
        );
        assert_eq!(state_hash(&db.conn)?, before);
    }
    Ok(())
}

#[test]
fn zip_header_limits_are_rejected_before_decompression() -> StoreResult<()> {
    let many: Vec<_> = (0..1001)
        .map(|i| (format!("assets/f{i}"), Vec::<u8>::new()))
        .collect();
    let mut z = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in many {
        z.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )?;
        z.write_all(&bytes)?;
    }
    let data = z.finish()?.into_inner();
    assert!(package::read_zip(&data)
        .unwrap_err()
        .to_string()
        .contains("1000"));
    fn with_sizes(size: u32, count: usize) -> StoreResult<Vec<u8>> {
        let names: Vec<_> = (0..count).map(|i| format!("assets/f{i}")).collect();
        let members: Vec<_> = names
            .iter()
            .map(|n| (n.as_str(), b"".as_slice(), None))
            .collect();
        let mut bytes = raw_zip(&members)?;
        let mut at = 0;
        while at + 46 <= bytes.len() {
            if bytes[at..at + 4] == [0x50, 0x4b, 0x01, 0x02] {
                bytes[at + 24..at + 28].copy_from_slice(&size.to_le_bytes());
                let name = u16::from_le_bytes([bytes[at + 28], bytes[at + 29]]) as usize;
                let extra = u16::from_le_bytes([bytes[at + 30], bytes[at + 31]]) as usize;
                let comment = u16::from_le_bytes([bytes[at + 32], bytes[at + 33]]) as usize;
                at += 46 + name + extra + comment;
            } else {
                at += 1;
            }
        }
        Ok(bytes)
    }
    assert!(package::read_zip(&with_sizes(51 * 1024 * 1024, 1)?)
        .unwrap_err()
        .to_string()
        .contains("50 MiB"));
    assert!(package::read_zip(&with_sizes(45 * 1024 * 1024, 6)?)
        .unwrap_err()
        .to_string()
        .contains("250 MiB"));
    Ok(())
}
#[test]
fn interrupted_publication_recovery_keeps_referenced_assets() -> StoreResult<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("knowledge.sqlite3");
    let db = Store::open(&path)?;
    let p = multimedia()?;
    let package = package::Package::read(&p.bytes, "zip", "knowledge_update")?;
    let staged = db.stage_assets(&package.assets)?;
    staged.publish()?;
    let keep = &package.assets[0].0;
    db.conn.execute(
        "INSERT INTO assets(id,filename,media_type,sha256,size) VALUES(?1,?2,?3,?4,?5)",
        rusqlite::params![
            keep.id,
            keep.filename,
            keep.media_type,
            keep.sha256,
            keep.size
        ],
    )?;
    std::mem::forget(staged);
    drop(db);
    let db = Store::open(&path)?;
    assert_eq!(db.read_asset(&keep.id)?, package.assets[0].1);
    let orphan = &package.assets[1].0;
    assert!(!root
        .path()
        .join(format!("assets/{}-{}", orphan.id, orphan.sha256))
        .exists());
    assert_eq!(std::fs::read_dir(root.path().join("assets"))?.count(), 1);
    Ok(())
}
#[test]
fn original_seed_reimport_with_reordered_keys_is_canonical_noop() -> StoreResult<()> {
    let root = tempfile::tempdir()?;
    let mut db = Store::open(&root.path().join("knowledge.sqlite3"))?;
    let value: Value =
        serde_json::from_str(include_str!("../resources/seed/sol-update-linux.json"))?;
    let p = ImportInput {
        format: "json".into(),
        mode: "knowledge_update".into(),
        bytes: canonical(&value).into_bytes(),
    };
    assert!(approve(&mut db, &p)?.preview.already_applied);
    assert!(db.list_backups()?.is_empty());
    Ok(())
}

#[test]
fn schema_three_raw_seed_hashes_upgrade_without_resetting_personal_content() -> StoreResult<()> {
    let root = tempfile::tempdir()?;
    let path = root.path().join("knowledge.sqlite3");
    let db = Store::open(&path)?;
    db.conn.execute("UPDATE entries SET payload_json=json_set(payload_json,'$.personal_description','Nota preservada na migração') WHERE id='linux-cmd-ls'",[])?;
    let seed: Value =
        serde_json::from_str(include_str!("../resources/seed/sol-update-linux.json"))?;
    for (id, bytes) in [
        (
            "builtin:settings:v2",
            include_bytes!("../resources/sol-settings.json").as_slice(),
        ),
        (
            seed["package_id"].as_str().unwrap(),
            include_bytes!("../resources/seed/sol-update-linux.json").as_slice(),
        ),
    ] {
        db.conn.execute(
            "UPDATE import_history SET sha256=?1,hash_format='raw' WHERE package_id=?2",
            rusqlite::params![hash(bytes), id],
        )?;
    }
    db.conn.execute_batch("DELETE FROM schema_migrations WHERE version=4; ALTER TABLE import_history DROP COLUMN hash_format;")?;
    drop(db);
    let mut db = Store::open(&path)?;
    assert_eq!(db.overview()?.schema_version, 4);
    assert_eq!(
        db.entry("linux-cmd-ls")?.payload["personal_description"],
        "Nota preservada na migração"
    );
    let p = ImportInput {
        format: "json".into(),
        mode: "knowledge_update".into(),
        bytes: canonical(&seed).into_bytes(),
    };
    assert!(approve(&mut db, &p)?.preview.already_applied);
    assert!(db.list_backups()?.is_empty());
    Ok(())
}
