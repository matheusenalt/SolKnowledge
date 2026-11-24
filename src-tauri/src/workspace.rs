use crate::{
    blocks::{validate_components, validate_id, COMPONENTS},
    models::*,
    storage::{Store, StoreResult},
};
use rusqlite::{params, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

const SCOPE: &str = "WITH RECURSIVE scope(id) AS (SELECT id FROM modules WHERE id=?2 UNION ALL SELECT m.id FROM modules m JOIN scope s ON m.parent_id=s.id)";
const FILTER: &str = "FROM entries e JOIN modules m ON m.id=e.module_id JOIN areas a ON a.id=m.area_id WHERE (?1 IS NULL OR a.id=?1) AND (?2 IS NULL OR m.id IN (SELECT id FROM scope)) AND (?3 IS NULL OR e.kind=?3) AND (?4 IS NULL OR e.learning_status=?4) AND (?5=0 OR EXISTS(SELECT 1 FROM favorites f WHERE f.entry_id=e.id))";

impl Store {
    pub fn catalog(&self) -> StoreResult<Catalog> {
        fn read(conn: &rusqlite::Connection, table: &str) -> StoreResult<Vec<Value>> {
            let sql = match table {
                "areas" => "SELECT a.payload_json,a.revision,(SELECT count(*) FROM entries e JOIN modules m ON m.id=e.module_id WHERE m.area_id=a.id) FROM areas a ORDER BY coalesce(json_extract(a.payload_json,'$.order'),0),a.name",
                "modules" => "SELECT m.payload_json,m.revision,(SELECT count(*) FROM entries e WHERE e.module_id=m.id) FROM modules m ORDER BY coalesce(json_extract(m.payload_json,'$.order'),0),m.name",
                _ => "SELECT payload_json,1,0 FROM profiles ORDER BY name",
            };
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?;
            rows.map(|row| {
                let (raw, revision, count) = row?;
                let mut v: Value = serde_json::from_str(&raw)?;
                v["revision"] = revision.into();
                v["entry_count"] = count.into();
                Ok(v)
            })
            .collect()
        }
        Ok(Catalog {
            areas: read(&self.conn, "areas")?,
            modules: read(&self.conn, "modules")?,
            profiles: read(&self.conn, "profiles")?,
            components: COMPONENTS.to_vec(),
        })
    }

    pub fn resolve_presentation(&self, module_id: &str) -> StoreResult<Presentation> {
        let area: String = self.conn.query_row(
            "SELECT area_id FROM modules WHERE id=?1",
            [module_id],
            |r| r.get(0),
        )?;
        let mut current = Some(module_id.to_owned());
        let mut visited = HashSet::new();
        while let Some(id) = current {
            if !visited.insert(id.clone()) {
                return Err("Ciclo na herança".into());
            }
            let (raw, profile, parent): (String, Option<String>, Option<String>) =
                self.conn.query_row(
                    "SELECT payload_json,profile_id,parent_id FROM modules WHERE id=?1",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )?;
            let payload: Value = serde_json::from_str(&raw)?;
            if let Some(list) = payload.get("enabled_components") {
                let components: Vec<String> = serde_json::from_value(list.clone())?;
                validate_components(&components)?;
                return Ok(Presentation {
                    profile_id: profile.unwrap_or_else(|| "custom".into()),
                    source: format!("module_components:{id}"),
                    components,
                });
            }
            if let Some(profile_id) = profile {
                return self.profile_presentation(&profile_id, format!("module_profile:{id}"));
            }
            current = parent;
        }
        let (raw, profile): (String, Option<String>) = self.conn.query_row(
            "SELECT payload_json,profile_id FROM areas WHERE id=?1",
            [&area],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let payload: Value = serde_json::from_str(&raw)?;
        if let Some(list) = payload.get("enabled_components") {
            let components: Vec<String> = serde_json::from_value(list.clone())?;
            validate_components(&components)?;
            return Ok(Presentation {
                profile_id: profile.unwrap_or_else(|| "custom".into()),
                source: format!("area_components:{area}"),
                components,
            });
        }
        self.profile_presentation(
            profile.as_deref().unwrap_or("custom"),
            format!("area:{area}"),
        )
    }
    fn profile_presentation(&self, id: &str, source: String) -> StoreResult<Presentation> {
        let raw: String =
            self.conn
                .query_row("SELECT payload_json FROM profiles WHERE id=?1", [id], |r| {
                    r.get(0)
                })?;
        let value: Value = serde_json::from_str(&raw)?;
        let components: Vec<String> = serde_json::from_value(value["enabled_components"].clone())?;
        validate_components(&components)?;
        Ok(Presentation {
            profile_id: id.into(),
            source,
            components,
        })
    }
    pub fn save_presentation(&mut self, input: PresentationInput) -> StoreResult<()> {
        if let Some(c) = &input.enabled_components {
            validate_components(c)?;
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let raw: String = tx.query_row(
            "SELECT payload_json FROM modules WHERE id=?1 AND revision=?2",
            params![input.module_id, input.expected_revision],
            |r| r.get(0),
        )?;
        let mut payload: Value = serde_json::from_str(&raw)?;
        let object = payload.as_object_mut().ok_or("Módulo inválido")?;
        match &input.profile_id {
            Some(p) => {
                object.insert("profile_id".into(), p.clone().into());
            }
            None => {
                object.remove("profile_id");
            }
        }
        match &input.enabled_components {
            Some(c) => {
                object.insert("enabled_components".into(), serde_json::to_value(c)?);
            }
            None => {
                object.remove("enabled_components");
            }
        }
        tx.execute(
            "UPDATE modules SET profile_id=?1,payload_json=?2,revision=revision+1 WHERE id=?3",
            params![input.profile_id, payload.to_string(), input.module_id],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn create_profile(&mut self, raw: &str) -> StoreResult<()> {
        #[derive(Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Profile {
            id: String,
            name: String,
            enabled_components: Vec<String>,
        }
        if raw.len() > 64 * 1024 {
            return Err("Perfil JSON muito grande".into());
        }
        let p: Profile = serde_json::from_str(raw)?;
        validate_id(&p.id)?;
        validate_name(&p.name)?;
        validate_components(&p.enabled_components)?;
        self.conn.execute(
            "INSERT INTO profiles(id,name,payload_json) VALUES(?1,?2,?3)",
            params![p.id, p.name, serde_json::to_string(&p)?],
        )?;
        Ok(())
    }
    pub fn create_area(&mut self, input: NewArea) -> StoreResult<()> {
        validate_id(&input.id)?;
        validate_name(&input.name)?;
        let mut payload = json!({"id":input.id,"name":input.name});
        if let Some(p) = &input.profile_id {
            payload["profile_id"] = p.clone().into();
        }
        self.conn.execute(
            "INSERT INTO areas(id,name,profile_id,payload_json) VALUES(?1,?2,?3,?4)",
            params![input.id, input.name, input.profile_id, payload.to_string()],
        )?;
        Ok(())
    }
    pub fn create_module(&mut self, input: NewModule) -> StoreResult<()> {
        validate_id(&input.id)?;
        validate_name(&input.name)?;
        let mut payload = json!({"id":input.id,"name":input.name,"area_id":input.area_id,"parent_id":input.parent_id});
        if let Some(p) = &input.profile_id {
            payload["profile_id"] = p.clone().into();
        }
        self.conn.execute("INSERT INTO modules(id,name,area_id,parent_id,profile_id,payload_json) VALUES(?1,?2,?3,?4,?5,?6)",params![input.id,input.name,input.area_id,input.parent_id,input.profile_id,payload.to_string()])?;
        Ok(())
    }
    pub(crate) fn matching_entry_ids(&self, f: &CatalogFilter) -> StoreResult<HashSet<String>> {
        let mut stmt = self
            .conn
            .prepare(&format!("{SCOPE} SELECT e.id {FILTER}"))?;
        let rows = stmt.query_map(
            params![
                f.area_id,
                f.module_id,
                f.entry_type,
                f.learning_status,
                f.favorites_only
            ],
            |r| r.get(0),
        )?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
    pub fn browse(&self, input: BrowseRequest) -> StoreResult<CatalogPage> {
        let f = &input.filters;
        let total = self.conn.query_row(
            &format!("{SCOPE} SELECT count(*) {FILTER}"),
            params![
                f.area_id,
                f.module_id,
                f.entry_type,
                f.learning_status,
                f.favorites_only
            ],
            |r| r.get(0),
        )?;
        let mut stmt=self.conn.prepare(&format!("{SCOPE} SELECT e.id,e.title,e.kind,a.id,a.name,m.id,m.name,coalesce(json_extract(e.payload_json,'$.technical_description'),''),e.learning_status,EXISTS(SELECT 1 FROM favorites WHERE entry_id=e.id),EXISTS(SELECT 1 FROM blocks b WHERE b.entry_id=e.id AND b.kind IN ('diagram','topology','image','attachment')) {FILTER} ORDER BY e.title,e.id LIMIT 40 OFFSET ?6"))?;
        let rows = stmt.query_map(
            params![
                f.area_id,
                f.module_id,
                f.entry_type,
                f.learning_status,
                f.favorites_only,
                input.offset
            ],
            |r| {
                Ok(SearchResult {
                    visual: r.get(10)?,
                    id: r.get(0)?,
                    title: r.get(1)?,
                    kind: r.get(2)?,
                    area_id: r.get(3)?,
                    area_name: r.get(4)?,
                    module_id: r.get(5)?,
                    module_name: r.get(6)?,
                    snippet: r.get(7)?,
                    learning_status: r.get(8)?,
                    favorite: r.get(9)?,
                    match_kind: MatchKind::Textual,
                })
            },
        )?;
        Ok(CatalogPage {
            items: rows.collect::<rusqlite::Result<_>>()?,
            total,
        })
    }
    pub fn favorite(&mut self, id: &str, value: bool) -> StoreResult<()> {
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE id=?1)",
            [id],
            |r| r.get(0),
        )?;
        if !exists {
            return Err("Registro não encontrado".into());
        }
        if value {
            self.conn
                .execute("INSERT OR IGNORE INTO favorites(entry_id) VALUES(?1)", [id])?;
        } else {
            self.conn
                .execute("DELETE FROM favorites WHERE entry_id=?1", [id])?;
        }
        Ok(())
    }
    pub fn visit_entry(&mut self, id: &str) -> StoreResult<EntryDetail> {
        let entry = self.entry(id)?;
        self.conn
            .execute("INSERT INTO visit_history(entry_id) VALUES(?1)", [id])?;
        Ok(entry)
    }
    pub fn history(&self) -> StoreResult<History> {
        let mut visits=self.conn.prepare("SELECT h.entry_id,e.title,h.visited_at FROM visit_history h JOIN entries e ON e.id=h.entry_id ORDER BY h.id DESC LIMIT 100")?;
        let visits=visits.query_map([],|r|Ok(json!({"entry_id":r.get::<_,String>(0)?,"title":r.get::<_,String>(1)?,"at":r.get::<_,String>(2)?})))?.collect::<rusqlite::Result<_>>()?;
        let mut searches = self
            .conn
            .prepare("SELECT query,created_at FROM search_history ORDER BY id DESC LIMIT 100")?;
        let searches = searches
            .query_map([], |r| {
                Ok(json!({"query":r.get::<_,String>(0)?,"at":r.get::<_,String>(1)?}))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(History { visits, searches })
    }
}
fn validate_name(name: &str) -> StoreResult<()> {
    if name.trim().is_empty() || name.chars().count() > 160 || name.chars().any(char::is_control) {
        return Err("Nome inválido (1 a 160 caracteres)".into());
    }
    Ok(())
}
