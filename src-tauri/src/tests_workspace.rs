use crate::{models::*, storage::Store};
use serde_json::{json, Value};

fn revision(db: &Store, id: &str) -> i64 {
    db.conn
        .query_row("SELECT revision FROM modules WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .unwrap()
}
fn present(db: &mut Store, id: &str, profile: Option<&str>, components: Option<Vec<&str>>) {
    db.save_presentation(PresentationInput {
        module_id: id.into(),
        expected_revision: revision(db, id),
        profile_id: profile.map(str::to_owned),
        enabled_components: components.map(|c| c.into_iter().map(str::to_owned).collect()),
    })
    .unwrap();
}
fn create_entry(db: &mut Store, id: &str, module: &str) -> EntryDetail {
    db.save_entry(EntryInput {
        id: id.into(),
        expected_revision: None,
        patch: json!({"module_id":module,"title":"Registro de teste isolado","type":"note"}),
        confirm_personal: false,
    })
    .unwrap()
}
fn block(db: &mut Store, id: &str, patch: Value, confirmed: bool) -> EntryDetail {
    let revision = db.entry(id).unwrap().revision;
    db.save_block(BlockInput {
        entry_id: id.into(),
        expected_revision: revision,
        patch,
        confirm_personal: confirmed,
    })
    .unwrap()
}
fn query(db: &mut Store, q: &str) -> Vec<SearchResult> {
    db.search(SearchRequest {
        query: q.into(),
        limit: None,
        filters: None,
    })
    .unwrap()
}

#[test]
fn presentation_precedence_and_explicit_empty_components() {
    let mut db = Store::memory().unwrap();
    db.create_area(NewArea {
        id: "test-area".into(),
        name: "Fixture".into(),
        profile_id: None,
    })
    .unwrap();
    db.create_module(NewModule {
        id: "test-root".into(),
        name: "Raiz".into(),
        area_id: "test-area".into(),
        parent_id: None,
        profile_id: None,
    })
    .unwrap();
    db.create_module(NewModule {
        id: "test-child".into(),
        name: "Filho".into(),
        area_id: "test-area".into(),
        parent_id: Some("test-root".into()),
        profile_id: None,
    })
    .unwrap();
    assert_eq!(
        db.resolve_presentation("test-child").unwrap().profile_id,
        "custom"
    );
    db.conn
        .execute(
            "UPDATE areas SET profile_id='linux' WHERE id='test-area'",
            [],
        )
        .unwrap();
    assert_eq!(
        db.resolve_presentation("test-child").unwrap().profile_id,
        "linux"
    );
    present(&mut db, "test-root", Some("cloud"), None);
    assert_eq!(
        db.resolve_presentation("test-child").unwrap().profile_id,
        "cloud"
    );
    present(&mut db, "test-child", Some("redes"), None);
    assert_eq!(
        db.resolve_presentation("test-child").unwrap().profile_id,
        "redes"
    );
    present(
        &mut db,
        "test-child",
        Some("redes"),
        Some(vec!["code", "personal_note"]),
    );
    assert_eq!(
        db.resolve_presentation("test-child").unwrap().components,
        vec!["code", "personal_note"]
    );
    present(&mut db, "test-child", Some("redes"), Some(vec![]));
    assert!(db
        .resolve_presentation("test-child")
        .unwrap()
        .components
        .is_empty());
    present(&mut db, "test-child", None, None);
    assert_eq!(
        db.resolve_presentation("test-child").unwrap().profile_id,
        "cloud"
    );
    let revision = revision(&db, "test-child");
    assert!(db
        .save_presentation(PresentationInput {
            module_id: "test-child".into(),
            expected_revision: revision - 1,
            profile_id: None,
            enabled_components: None
        })
        .is_err());
    assert!(db
        .save_presentation(PresentationInput {
            module_id: "test-child".into(),
            expected_revision: revision,
            profile_id: Some("missing".into()),
            enabled_components: None
        })
        .is_err());
    assert_eq!(
        db.resolve_presentation("test-child").unwrap().profile_id,
        "cloud"
    );
}

#[test]
fn profile_json_is_data_only_and_never_overwrites_existing_profiles() {
    let mut db = Store::memory().unwrap();
    db.create_profile(
        r#"{"id":"fixture-profile","name":"Teste","enabled_components":["code","text"]}"#,
    )
    .unwrap();
    present(&mut db, "linux-02", Some("fixture-profile"), None);
    assert_eq!(
        db.resolve_presentation("linux-02").unwrap().components,
        vec!["code", "text"]
    );
    for bad in [
        r#"{"id":"linux","name":"Overwrite","enabled_components":[]}"#,
        r#"{"id":"bad","name":"Teste","enabled_components":["javascript"]}"#,
        r#"{"id":"bad","name":"Teste","enabled_components":["text","text"]}"#,
        r#"{"id":"bad","name":"Teste","enabled_components":[],"script":"alert(1)"}"#,
    ] {
        assert!(db.create_profile(bad).is_err());
    }
    assert_eq!(db.overview().unwrap().profiles, 10);
}

#[test]
fn block_patch_profile_changes_and_restart_preserve_all_content() {
    let path = std::env::temp_dir().join(format!(
        "sol-profiles-{}-{}.sqlite3",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let mut db = Store::open(&path).unwrap();
        create_entry(&mut db, "fixture-blocks", "linux-02");
        block(
            &mut db,
            "fixture-blocks",
            json!({"id":"code","type":"code","language":"bash","content":"apenas-texto-de-fixture"}),
            false,
        );
        block(
            &mut db,
            "fixture-blocks",
            json!({"id":"graph","type":"topology","format":"graph","nodes":[{"id":"pc","label":"PC fixture"}],"edges":[]}),
            false,
        );
        block(
            &mut db,
            "fixture-blocks",
            json!({"id":"personal","type":"personal_note","content":"notapessoalfixture"}),
            false,
        );
        let before = db.entry("fixture-blocks").unwrap();
        let revision = before.revision;
        assert!(db
            .save_block(BlockInput {
                entry_id: "fixture-blocks".into(),
                expected_revision: revision,
                patch: json!({"id":"personal","content":"outro"}),
                confirm_personal: false
            })
            .is_err());
        assert_eq!(db.entry("fixture-blocks").unwrap().revision, revision);
        block(
            &mut db,
            "fixture-blocks",
            json!({"id":"personal","content":"notarevisadafixture"}),
            true,
        );
        let revision = db.entry("fixture-blocks").unwrap().revision;
        assert!(db
            .save_block(BlockInput {
                entry_id: "fixture-blocks".into(),
                expected_revision: revision,
                patch: json!({"id":"personal","type":"text"}),
                confirm_personal: true
            })
            .is_err());
        let order = vec!["personal".into(), "graph".into(), "code".into()];
        db.reorder_blocks(BlockOrder {
            entry_id: "fixture-blocks".into(),
            expected_revision: revision,
            ids: order,
        })
        .unwrap();
        present(&mut db, "linux-02", Some("cloud"), Some(vec!["code"]));
        let after = db.entry("fixture-blocks").unwrap();
        assert_eq!(after.blocks.len(), 3);
        assert_eq!(after.blocks[0].payload["id"], "personal");
        assert_eq!(before.blocks[0].payload, after.blocks[2].payload);
        assert_eq!(before.blocks[1].payload, after.blocks[1].payload);
        assert_eq!(
            query(&mut db, "notarevisadafixture")[0].id,
            "fixture-blocks"
        );
        assert!(query(&mut db, "notapessoalfixture").is_empty());
    }
    {
        let mut db = Store::open(&path).unwrap();
        let detail = db.entry("fixture-blocks").unwrap();
        assert_eq!(detail.presentation.components, vec!["code"]);
        assert_eq!(detail.blocks.len(), 3);
        assert_eq!(detail.blocks[0].payload["content"], "notarevisadafixture");
        assert_eq!(
            query(&mut db, "notarevisadafixture")[0].id,
            "fixture-blocks"
        );
        assert_eq!(db.overview().unwrap().entries, 62);
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn v1_adapter_is_virtual_and_editor_preserves_omitted_fields() {
    let mut db = Store::memory().unwrap();
    let before = db.entry("linux-cmd-ls").unwrap();
    assert!(!before.blocks.is_empty());
    assert!(before.blocks.iter().all(|b| b.virtual_block));
    let count: i64 = db
        .conn
        .query_row("SELECT count(*) FROM blocks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
    let after = db
        .save_entry(EntryInput {
            id: before.id.clone(),
            expected_revision: Some(before.revision),
            patch: json!({"personal_description":"fixture-pessoal"}),
            confirm_personal: false,
        })
        .unwrap();
    assert_eq!(before.payload["options"], after.payload["options"]);
    assert_eq!(
        before.payload["technical_description"],
        after.payload["technical_description"]
    );
    assert!(db
        .save_entry(EntryInput {
            id: before.id.clone(),
            expected_revision: Some(after.revision),
            patch: json!({"personal_description":"mudança"}),
            confirm_personal: false
        })
        .is_err());
    assert!(db
        .save_entry(EntryInput {
            id: before.id,
            expected_revision: Some(before.revision),
            patch: json!({"title":"edição antiga"}),
            confirm_personal: true
        })
        .is_err());
    assert_eq!(
        db.entry("linux-cmd-ls").unwrap().payload["personal_description"],
        "fixture-pessoal"
    );
}

#[test]
fn browse_descendants_favorites_history_and_learning_are_explicit() {
    let mut db = Store::memory().unwrap();
    db.create_module(NewModule {
        id: "fixture-child".into(),
        name: "Filho".into(),
        area_id: "linux".into(),
        parent_id: Some("linux-01".into()),
        profile_id: None,
    })
    .unwrap();
    let e = create_entry(&mut db, "fixture-note", "fixture-child");
    db.favorite(&e.id, true).unwrap();
    db.visit_entry(&e.id).unwrap();
    assert_eq!(
        db.entry(&e.id).unwrap().payload["learning_status"],
        "not_assessed"
    );
    let f = CatalogFilter {
        module_id: Some("linux-01".into()),
        favorites_only: true,
        ..Default::default()
    };
    let page = db
        .browse(BrowseRequest {
            filters: f.clone(),
            offset: 0,
        })
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].id, e.id);
    let results = db
        .search(SearchRequest {
            query: "Registro de teste isolado".into(),
            limit: Some(1),
            filters: Some(f),
        })
        .unwrap();
    assert_eq!(results[0].id, e.id);
    let history = db.history().unwrap();
    assert_eq!(history.visits[0]["entry_id"], e.id);
    assert!(!history.searches.is_empty());
    let updated = db
        .save_entry(EntryInput {
            id: e.id.clone(),
            expected_revision: Some(e.revision),
            patch: json!({"learning_status":"studying"}),
            confirm_personal: false,
        })
        .unwrap();
    assert_eq!(updated.payload["learning_status"], "studying");
    db.favorite(&e.id, false).unwrap();
    assert!(!db.entry(&e.id).unwrap().favorite);
}

#[test]
fn invalid_blocks_do_not_write_or_advance_revision() {
    let mut db = Store::memory().unwrap();
    let e = create_entry(&mut db, "fixture-invalid", "linux-02");
    for bad in [
        json!({"id":"x","type":"javascript","content":"run"}),
        json!({"id":"x","type":"code","content":"missing language"}),
        json!({"id":"x","type":"table","columns":["A"],"rows":[["A","B"]]}),
        json!({"id":"x","type":"image","asset_id":"unknown"}),
        json!({"id":"x","type":"topology","format":"graph","nodes":[],"edges":[{"id":"edge","from":"unknown","to":"unknown"}]}),
    ] {
        assert!(db
            .save_block(BlockInput {
                entry_id: e.id.clone(),
                expected_revision: e.revision,
                patch: bad,
                confirm_personal: true
            })
            .is_err());
        assert_eq!(db.entry(&e.id).unwrap().revision, e.revision);
    }
}
