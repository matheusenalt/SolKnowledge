use crate::{models::*, storage::Store};
use serde_json::json;

fn path(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}.sqlite3",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn backend_process_restart_preserves_profile_blocks_and_search() {
    let path = path("sol-restart");
    for phase in ["write", "verify"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tests_restart::restart_worker",
                "--ignored",
                "--nocapture",
            ])
            .env("SOL_TEST_RESTART_PATH", &path)
            .env("SOL_TEST_RESTART_PHASE", phase)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "phase {phase}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
#[ignore = "Auxiliar executado em dois processos independentes pelo teste de reinício"]
fn restart_worker() {
    let path = std::path::PathBuf::from(std::env::var_os("SOL_TEST_RESTART_PATH").unwrap());
    let mut db = Store::open(&path).unwrap();
    if std::env::var("SOL_TEST_RESTART_PHASE").unwrap() == "write" {
        let entry=db.save_entry(EntryInput {id:"restart-fixture".into(),expected_revision:None,
            patch:json!({"module_id":"linux-02","title":"Fixture de reinício isolada","type":"note"}),confirm_personal:false}).unwrap();
        db.save_block(BlockInput {entry_id:entry.id,expected_revision:entry.revision,
            patch:json!({"id":"note","type":"personal_note","content":"persistenciaprocessofixture"}),confirm_personal:false}).unwrap();
        db.save_presentation(PresentationInput {
            module_id: "linux-02".into(),
            expected_revision: 1,
            profile_id: Some("cloud".into()),
            enabled_components: Some(vec!["code".into(), "text".into()]),
        })
        .unwrap();
        db.favorite("restart-fixture", true).unwrap();
    } else {
        let e = db.entry("restart-fixture").unwrap();
        assert_eq!(e.presentation.components, vec!["code", "text"]);
        assert_eq!(
            e.blocks[0].payload["content"],
            "persistenciaprocessofixture"
        );
        assert!(e.favorite);
        let results = db
            .search(SearchRequest {
                query: "persistenciaprocessofixture".into(),
                limit: None,
                filters: None,
            })
            .unwrap();
        assert_eq!(results[0].id, "restart-fixture");
        assert_eq!(db.overview().unwrap().entries, 62);
    }
}

#[test]
fn migration_from_version_one_preserves_personal_payload_and_fts() {
    let path = path("sol-migration");
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);").unwrap();
        conn.execute_batch(include_str!("../migrations/001_core.sql"))
            .unwrap();
        conn.execute_batch("INSERT INTO schema_migrations(version) VALUES(1);
            INSERT INTO areas(id,name,payload_json) VALUES('test-area','Fixture','{\"id\":\"test-area\",\"name\":\"Fixture\"}');
            INSERT INTO modules(id,area_id,name,payload_json) VALUES('test-module','test-area','Fixture','{\"id\":\"test-module\",\"area_id\":\"test-area\",\"name\":\"Fixture\"}');").unwrap();
        conn.execute("INSERT INTO entries(id,module_id,payload_json) VALUES('test-migration','test-module',?1)",
            [json!({"id":"test-migration","module_id":"test-module","title":"Fixture","type":"note","personal_description":"notamigracaoisolada"}).to_string()]).unwrap();
    }
    {
        let mut db = Store::open(&path).unwrap();
        assert_eq!(db.overview().unwrap().schema_version, 4);
        assert_eq!(
            db.entry("test-migration").unwrap().payload["personal_description"],
            "notamigracaoisolada"
        );
        let results = db
            .search(SearchRequest {
                query: "notamigracaoisolada".into(),
                limit: None,
                filters: None,
            })
            .unwrap();
        assert_eq!(results[0].id, "test-migration");
        let e = db.entry("test-migration").unwrap();
        let edited = db
            .save_entry(EntryInput {
                id: e.id,
                expected_revision: Some(e.revision),
                patch: json!({"title":"Título revisado"}),
                confirm_personal: false,
            })
            .unwrap();
        assert_eq!(
            edited.payload["personal_description"],
            "notamigracaoisolada"
        );
        assert_eq!(edited.payload["learning_status"], "not_assessed");
        db.visit_entry("test-migration").unwrap();
        assert_eq!(db.history().unwrap().visits.len(), 1);
    }
    std::fs::remove_file(path).unwrap();
}
