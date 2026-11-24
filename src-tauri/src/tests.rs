use crate::{models::*, storage::Store};
use serde_json::json;

fn search(db: &mut Store, query: &str) -> Vec<SearchResult> {
    db.search(SearchRequest {
        query: query.into(),
        limit: Some(100),
        filters: None,
    })
    .unwrap()
}

#[test]
fn seed_idempotent_preserves_reference_and_personal_edits() {
    let mut db = Store::memory().unwrap();
    let c = db.overview().unwrap();
    assert_eq!(
        (c.entries, c.options, c.linux_modules, c.areas, c.profiles),
        (61, 205, 24, 8, 9)
    );
    for n in 1..=24 {
        assert!(db
            .conn
            .query_row(
                "SELECT 1 FROM modules WHERE id=?1",
                [format!("linux-{n:02}")],
                |r| r.get::<_, i32>(0)
            )
            .is_ok());
    }
    let invalid: i64 = db.conn.query_row("SELECT count(*) FROM entries WHERE options_coverage <> 'selected' OR learning_status <> 'not_assessed'",[],|r|r.get(0)).unwrap();
    assert_eq!(invalid, 0);
    assert_eq!(
        db.vocabulary()
            .unwrap()
            .iter()
            .filter(|t| matches!(t.kind, TermKind::Personal))
            .count(),
        0
    );
    db.conn.execute("UPDATE entries SET payload_json=json_set(payload_json,'$.personal_description','nota de teste','$.learning_status','studying') WHERE id='linux-cmd-ls'",[]).unwrap();
    db.seed().unwrap();
    db.seed().unwrap();
    assert_eq!(db.overview().unwrap().entries, 61);
    assert_eq!(db.overview().unwrap().options, 205);
    assert_eq!(
        db.entry("linux-cmd-ls").unwrap().payload["personal_description"],
        "nota de teste"
    );
    assert_eq!(
        db.entry("linux-cmd-ls").unwrap().payload["learning_status"],
        "studying"
    );
    let imports: i64 = db
        .conn
        .query_row("SELECT count(*) FROM import_history", [], |r| r.get(0))
        .unwrap();
    let fts: i64 = db
        .conn
        .query_row("SELECT count(*) FROM entries_fts", [], |r| r.get(0))
        .unwrap();
    assert_eq!((imports, fts), (2, 61));
}

#[test]
fn required_queries_rank_with_snippet_and_location() {
    let mut db = Store::memory().unwrap();
    for (query, id, kind) in [
        ("ls -lh", "linux-cmd-ls", MatchKind::Exact),
        ("sistemctl", "linux-cmd-systemctl", MatchKind::Approximate),
        ("arquivos escondidos", "linux-cmd-ls", MatchKind::Textual),
        (
            "ver quanto espaço sobra no HD",
            "linux-cmd-df",
            MatchKind::Textual,
        ),
        (
            "ver quanto espaco sobra no hd",
            "linux-cmd-df",
            MatchKind::Textual,
        ),
        ("ficheiros ocultos", "linux-cmd-ls", MatchKind::Textual),
    ] {
        let results = search(&mut db, query);
        let top = results
            .first()
            .unwrap_or_else(|| panic!("Sem resultado: {query}"));
        assert_eq!(top.id, id, "{query}: {results:?}");
        assert_eq!(top.match_kind, kind, "{query}");
        assert_eq!(top.area_name, "Linux");
        assert!(!top.module_name.is_empty() && !top.snippet.is_empty());
        assert!(results
            .windows(2)
            .all(|p| p[0].match_kind <= p[1].match_kind));
    }
}

#[test]
fn single_character_prefix_is_fts_bounded_scoped_and_safe() {
    let mut db = Store::memory().unwrap();
    let results = search(&mut db, "L");
    assert!(results.iter().any(|r| r.id == "linux-cmd-ls"));
    assert!(results.iter().any(|r| r.match_kind == MatchKind::Textual));
    assert!(!search(&mut db, "a").is_empty());
    for query in ["", "   ", "*", "\"", "()", ":", "💛"] {
        assert!(search(&mut db, query).is_empty(), "{query}");
    }
    let limited = db
        .search(SearchRequest {
            query: "L".into(),
            limit: Some(1),
            filters: None,
        })
        .unwrap();
    assert_eq!(limited.len(), 1);
    let scoped = db
        .search(SearchRequest {
            query: "L".into(),
            limit: Some(100),
            filters: Some(CatalogFilter {
                area_id: Some("missing-area".into()),
                ..Default::default()
            }),
        })
        .unwrap();
    assert!(scoped.is_empty());
}

#[test]
fn vocabulary_create_edit_remove_and_invalid_target() {
    let mut db = Store::memory().unwrap();
    let phrase = "minha expressão de fixture";
    assert!(search(&mut db, phrase).is_empty());
    let id = db
        .save_term(TermInput {
            id: None,
            kind: TermKind::Personal,
            phrase: phrase.into(),
            target_entry_id: "linux-cmd-df".into(),
        })
        .unwrap();
    assert_eq!(search(&mut db, phrase)[0].id, "linux-cmd-df");
    assert!(db
        .save_term(TermInput {
            id: None,
            kind: TermKind::Personal,
            phrase: phrase.into(),
            target_entry_id: "linux-cmd-df".into()
        })
        .is_err());
    db.save_term(TermInput {
        id: Some(id),
        kind: TermKind::Personal,
        phrase: phrase.into(),
        target_entry_id: "linux-cmd-ls".into(),
    })
    .unwrap();
    assert_eq!(search(&mut db, phrase)[0].id, "linux-cmd-ls");
    assert!(db
        .save_term(TermInput {
            id: Some(id),
            kind: TermKind::Personal,
            phrase: "alterado".into(),
            target_entry_id: "ausente".into()
        })
        .is_err());
    assert_eq!(search(&mut db, phrase)[0].id, "linux-cmd-ls");
    db.delete_term(TermKind::Personal, id).unwrap();
    assert!(search(&mut db, phrase).is_empty());
    assert!(db
        .save_term(TermInput {
            id: None,
            kind: TermKind::Personal,
            phrase: " ".into(),
            target_entry_id: "linux-cmd-ls".into()
        })
        .is_err());
}

#[test]
fn fts_tracks_entry_and_block_writes_and_rollback() {
    let mut db = Store::memory().unwrap();
    let tx = db.conn.transaction().unwrap();
    tx.execute("UPDATE entries SET payload_json=json_set(payload_json,'$.personal_description','quasartransacional') WHERE id='linux-cmd-ls'",[]).unwrap();
    let found: i64 = tx
        .query_row(
            "SELECT count(*) FROM entries_fts WHERE entries_fts MATCH 'quasartransacional'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(found, 1);
    tx.rollback().unwrap();
    assert!(search(&mut db, "quasartransacional").is_empty());
    let tx = db.conn.transaction().unwrap();
    tx.execute("INSERT INTO blocks(entry_id,id,kind,position,payload_json) VALUES('linux-cmd-ls','note','personal_note',0,?1)",
        [json!({"id":"note","type":"personal_note","content":"blocoocultoindexado"}).to_string()]).unwrap();
    tx.commit().unwrap();
    assert_eq!(search(&mut db, "blocoocultoindexado")[0].id, "linux-cmd-ls");
    assert!(db
        .conn
        .execute(
            "UPDATE blocks SET kind='text' WHERE entry_id='linux-cmd-ls' AND id='note'",
            []
        )
        .is_err());
    db.conn
        .execute(
            "UPDATE blocks SET payload_json=?1 WHERE entry_id='linux-cmd-ls' AND id='note'",
            [
                json!({"id":"note","type":"personal_note","content":"blocorevisadoindexado"})
                    .to_string(),
            ],
        )
        .unwrap();
    assert!(search(&mut db, "blocoocultoindexado").is_empty());
    assert_eq!(
        search(&mut db, "blocorevisadoindexado")[0].id,
        "linux-cmd-ls"
    );
    db.conn
        .execute(
            "DELETE FROM blocks WHERE entry_id='linux-cmd-ls' AND id='note'",
            [],
        )
        .unwrap();
    assert!(search(&mut db, "blocorevisadoindexado").is_empty());
}

#[test]
fn hierarchy_rejects_cycles_and_cross_area_parents() {
    let mut db = Store::memory().unwrap();
    db.conn
        .execute(
            "UPDATE modules SET parent_id='linux-01' WHERE id='linux-02'",
            [],
        )
        .unwrap();
    assert!(db
        .conn
        .execute(
            "UPDATE modules SET parent_id='linux-02' WHERE id='linux-01'",
            []
        )
        .is_err());
    assert!(db
        .conn
        .execute("UPDATE modules SET parent_id=id WHERE id='linux-01'", [])
        .is_err());
    assert!(db
        .conn
        .execute(
            "UPDATE modules SET parent_id='ausente' WHERE id='linux-01'",
            []
        )
        .is_err());
    assert!(db
        .conn
        .execute(
            "UPDATE modules SET parent_id='linux-01' WHERE area_id <> 'linux'",
            []
        )
        .is_err());
    assert!(db.conn.execute("INSERT INTO modules(id,area_id,parent_id,name,payload_json) VALUES('cycle','linux','cycle','test','{}')",[]).is_err());
    let tx = db.conn.transaction().unwrap();
    tx.execute("INSERT INTO modules(id,area_id,parent_id,name,payload_json) VALUES('a','linux','b','a','{}')",[]).unwrap();
    assert!(tx.execute("INSERT INTO modules(id,area_id,parent_id,name,payload_json) VALUES('b','linux','a','b','{}')",[]).is_err());
    tx.rollback().unwrap();
}

#[test]
fn safe_queries_and_limits() {
    let mut db = Store::memory().unwrap();
    for q in ["", "   ", "\" OR * NEAR(' --", "zzzzzzzzzzzzzzzzzzzz"] {
        assert!(search(&mut db, q).is_empty(), "{q}");
    }
    assert!(db
        .search(SearchRequest {
            query: "a".repeat(257),
            limit: None,
            filters: None
        })
        .is_err());
    assert!(db
        .search(SearchRequest {
            query: "ls\0x".into(),
            limit: None,
            filters: None
        })
        .is_err());
    assert!(
        db.search(SearchRequest {
            query: "arquivo".into(),
            limit: Some(1),
            filters: None
        })
        .unwrap()
        .len()
            <= 1
    );
    assert_eq!(db.overview().unwrap().entries, 61);
}

#[test]
fn reopen_keeps_database_migrations_history_and_vocabulary() {
    let path = std::env::temp_dir().join(format!(
        "sol-core-test-{}-{}.sqlite3",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let mut db = Store::open(&path).unwrap();
        db.save_term(TermInput {
            id: None,
            kind: TermKind::Personal,
            phrase: "fixture persistente".into(),
            target_entry_id: "linux-cmd-df".into(),
        })
        .unwrap();
        db.conn
            .execute("INSERT INTO favorites(entry_id) VALUES('linux-cmd-ls')", [])
            .unwrap();
        search(&mut db, "ls -lh");
    }
    {
        let mut db = Store::open(&path).unwrap();
        assert_eq!(db.overview().unwrap().entries, 61);
        assert_eq!(search(&mut db, "fixture persistente")[0].id, "linux-cmd-df");
        let migrations: i64 = db
            .conn
            .query_row("SELECT count(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        let favorites: i64 = db
            .conn
            .query_row("SELECT count(*) FROM favorites", [], |r| r.get(0))
            .unwrap();
        let history: i64 = db
            .conn
            .query_row(
                "SELECT count(*) FROM search_history WHERE query='ls -lh'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!((migrations, favorites, history), (4, 1, 1));
        db.conn
            .execute("INSERT INTO schema_migrations(version) VALUES(999)", [])
            .unwrap();
        db.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
    }
    assert!(Store::open(&path).is_err());
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        let entries: i64 = conn
            .query_row("SELECT count(*) FROM entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(entries, 61);
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .unwrap();
    }
    std::fs::remove_file(&path).unwrap();
}

#[test]
fn exact_precedes_textual_and_approximate_in_a_mixed_result() {
    let mut db = Store::memory().unwrap();
    for (id, title, description) in [
        ("fixture-textual", "Consulta de serviços", "systemctl"),
        ("fixture-fuzzy", "systemct1", "referência de teste"),
    ] {
        let payload = json!({"id":id,"module_id":"linux-07","type":"note","title":title,"technical_description":description});
        db.conn
            .execute(
                "INSERT INTO entries(id,module_id,payload_json) VALUES(?1,'linux-07',?2)",
                rusqlite::params![id, payload.to_string()],
            )
            .unwrap();
    }
    let results = search(&mut db, "systemctl");
    assert_eq!(results[0].id, "linux-cmd-systemctl");
    let text = results
        .iter()
        .position(|r| r.id == "fixture-textual")
        .unwrap();
    let fuzzy = results
        .iter()
        .position(|r| r.id == "fixture-fuzzy")
        .unwrap();
    assert!(text < fuzzy);
    assert_eq!(results[text].match_kind, MatchKind::Textual);
    assert_eq!(results[fuzzy].match_kind, MatchKind::Approximate);
}

#[test]
fn changed_package_preserves_existing_data() {
    let mut db = Store::memory().unwrap();
    db.conn
        .execute(
            "UPDATE import_history SET sha256=?1 WHERE kind='seed'",
            ["0".repeat(64)],
        )
        .unwrap();
    assert!(db.seed().is_err());
    assert_eq!(db.overview().unwrap().entries, 61);
}
