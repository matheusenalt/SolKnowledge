//! Incremental merges are rehearsed on an SQLite backup clone before the live transaction.
use crate::{
    blocks::{validate_block, validate_components, validate_entry, validate_id},
    package::{self, canonical, hash, keys, list, text, Package},
    storage::{Store, StoreResult},
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    time::Duration,
};
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Change {
    pub id: String,
    pub before: Value,
    pub after: Value,
    pub requires_approval: bool,
}
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Counts {
    pub new: u64,
    pub updated: u64,
    pub same: u64,
    pub conflicts: u64,
}
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct Preview {
    pub package_id: String,
    pub kind: String,
    pub sha256: String,
    pub review_token: String,
    pub already_applied: bool,
    pub counts: Counts,
    pub changes: Vec<Change>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportInput {
    pub format: String,
    pub mode: String,
    pub bytes: Vec<u8>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportApproval {
    pub review_token: String,
    pub approved_changes: Vec<String>,
    pub confirm: bool,
}
#[derive(Serialize)]
pub struct Applied {
    pub preview: Preview,
    pub backup_id: Option<String>,
}
impl Preview {
    fn diff(&mut self, path: &str, before: Option<&Value>, after: &Value) {
        if before == Some(after) {
            self.counts.same += 1;
            return;
        }
        if before.is_none() {
            self.counts.new += 1;
        } else {
            self.counts.updated += 1;
        }
        let empty = json!({});
        let old = before.unwrap_or(&empty);
        for (k, new) in after.as_object().into_iter().flatten() {
            if old.get(k) == Some(new) {
                continue;
            }
            let special = k == "personal_description"
                || k == "learning_status"
                || (after["type"] == "personal_note" && k == "content");
            let requires = old.get(k).is_some()
                || special
                    && !new
                        .as_str()
                        .is_some_and(|s| s.is_empty() || s == "not_assessed");
            self.changes.push(Change {
                id: format!("{path}/{k}"),
                before: old.get(k).cloned().unwrap_or(Value::Null),
                after: new.clone(),
                requires_approval: requires,
            });
        }
    }
}
fn schema_check(v: &Value, s: &Value, path: &str) -> StoreResult<()> {
    if let Some(a) = s["anyOf"].as_array() {
        if a.iter().any(|s| schema_check(v, s, path).is_ok()) {
            return Ok(());
        }
        return Err(format!("{path}: tipo incompatível").into());
    }
    if let Some(c) = s.get("const") {
        if c != v {
            return Err(format!("{path}: valor fixo incompatível").into());
        }
    }
    if let Some(a) = s["enum"].as_array() {
        if !a.contains(v) {
            return Err(format!("{path}: valor desconhecido").into());
        }
    }
    if let Some(t) = s["type"].as_str() {
        let valid = match t {
            "object" => v.is_object(),
            "array" => v.is_array(),
            "string" => v.is_string(),
            "integer" => v.is_i64() || v.is_u64(),
            "boolean" => v.is_boolean(),
            "null" => v.is_null(),
            _ => false,
        };
        if !valid {
            return Err(format!("{path}: esperado {t}").into());
        }
    }
    if s.get("pattern").is_some() {
        validate_id(v.as_str().ok_or("ID deve ser texto")?)?;
    }
    if let Some(obj) = v.as_object() {
        let props = s["properties"].as_object();
        if let Some(req) = s["required"].as_array() {
            for k in req {
                if !obj.contains_key(k.as_str().ok_or("Schema inválido")?) {
                    return Err(format!("{path}: campo obrigatório {k}").into());
                }
            }
        }
        for (k, value) in obj {
            if let Some(sub) = props.and_then(|p| p.get(k)) {
                schema_check(value, sub, &format!("{path}/{k}"))?;
            } else if s["additionalProperties"] == false {
                return Err(format!("{path}: campo desconhecido {k}").into());
            }
        }
    }
    if let Some(a) = v.as_array() {
        if a.len() > 10000 {
            return Err("Lista excede limite".into());
        }
        let mut seen = HashSet::new();
        for (i, item) in a.iter().enumerate() {
            if s["uniqueItems"] == true && !seen.insert(canonical(item)) {
                return Err("Itens duplicados".into());
            }
            if let Some(sub) = s.get("items") {
                schema_check(item, sub, &format!("{path}/{i}"))?;
            }
        }
    }
    Ok(())
}
fn validate_package_schema(p: &Package) -> StoreResult<()> {
    let mut schema: Value = serde_json::from_str(if p.kind == "settings" {
        include_str!("../../schemas/settings.schema.json")
    } else {
        include_str!("../../schemas/update.schema.json")
    })?;
    if p.kind == "knowledge_update" {
        schema["properties"]["entries"]["items"]["properties"]["blocks"]["items"]["properties"]
            ["preview_asset_id"] = json!({"type":"string","pattern":"id"});
    }
    schema_check(&p.value, &schema, "pacote")
}
fn merge(old: &Value, patch: &Value) -> StoreResult<Value> {
    let mut result = old.as_object().ok_or("Objeto existente inválido")?.clone();
    for (k, v) in patch.as_object().ok_or("Patch deve ser objeto")? {
        if v.is_null() && k != "parent_id" {
            return Err(format!("null não permitido: {k}").into());
        }
        result.insert(k.clone(), v.clone());
    }
    Ok(Value::Object(result))
}
fn indexed(conn: &Connection, table: &str) -> StoreResult<BTreeMap<String, Value>> {
    let sql = match table {
        "profiles" => "SELECT id,payload_json FROM profiles",
        "areas" => "SELECT id,payload_json FROM areas",
        "modules" => "SELECT id,payload_json FROM modules",
        _ => return Err("Coleção inválida".into()),
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    rows.map(|r| {
        let (id, p) = r?;
        Ok((id, serde_json::from_str(&p)?))
    })
    .collect()
}
fn entity_list<'a>(p: &'a Value, k: &str) -> StoreResult<Vec<&'a Value>> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for v in list(p, k)? {
        let id = text(v, "id")?;
        validate_id(id)?;
        if !seen.insert(id) {
            return Err(format!("ID duplicado em {k}: {id}").into());
        }
        out.push(v);
    }
    Ok(out)
}
fn hierarchy_entity(v: &Value, kind: &str) -> StoreResult<()> {
    keys(
        v,
        match kind {
            "profiles" => &["id", "name", "enabled_components"],
            "areas" => &[
                "id",
                "name",
                "icon",
                "order",
                "profile_id",
                "enabled_components",
            ],
            _ => &[
                "id",
                "name",
                "area_id",
                "parent_id",
                "order",
                "profile_id",
                "enabled_components",
            ],
        },
    )?;
    validate_id(text(v, "id")?)?;
    if text(v, "name")?.trim().is_empty() {
        return Err("Nome obrigatório".into());
    }
    if let Some(a) = v.get("enabled_components") {
        validate_components(&serde_json::from_value::<Vec<String>>(a.clone())?)?;
    } else if kind == "profiles" {
        return Err("Perfil novo exige componentes".into());
    }
    for k in ["area_id", "profile_id"] {
        if v.get(k).is_some() {
            validate_id(text(v, k)?)?;
        }
    }
    if kind == "modules" {
        validate_id(text(v, "area_id")?)?;
        if let Some(p) = v.get("parent_id").filter(|v| !v.is_null()) {
            validate_id(p.as_str().ok_or("parent_id inválido")?)?;
        }
    }
    if v.get("icon").is_some_and(|v| !v.is_string())
        || v.get("order").is_some_and(|v| !v.is_i64() && !v.is_u64())
    {
        return Err("Ícone/ordem inválidos".into());
    }
    Ok(())
}
fn module_order(modules: &BTreeMap<String, Value>) -> StoreResult<Vec<String>> {
    let mut result = Vec::new();
    let mut done = HashSet::new();
    for start in modules.keys() {
        let mut chain = Vec::new();
        let mut local = HashSet::new();
        let mut current = Some(start.as_str());
        while let Some(id) = current {
            if done.contains(id) {
                break;
            }
            if !local.insert(id.to_owned()) {
                return Err("Ciclo na hierarquia final".into());
            }
            let v = modules.get(id).ok_or("Pai de módulo inexistente")?;
            chain.push(id.to_owned());
            current = v["parent_id"].as_str();
            if let Some(parent) = current {
                let pv = modules.get(parent).ok_or("Pai de módulo inexistente")?;
                if pv["area_id"] != v["area_id"] {
                    return Err("Pai precisa pertencer à mesma área".into());
                }
            }
        }
        for id in chain.into_iter().rev() {
            done.insert(id.clone());
            result.push(id);
        }
    }
    Ok(result)
}
fn old_json(conn: &Connection, sql: &str, id: &str) -> StoreResult<Option<Value>> {
    let s: Option<String> = conn.query_row(sql, [id], |r| r.get(0)).optional()?;
    s.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
}
fn merge_entry(old: &Value, patch: &Value) -> StoreResult<Value> {
    let mut p = patch.clone();
    p.as_object_mut()
        .ok_or("Registro inválido")?
        .remove("blocks");
    let mut value = merge(old, &p)?;
    for k in ["keywords", "sources", "related_entries"] {
        if let Some(incoming) = p.get(k) {
            let mut a = Vec::new();
            for item in old[k]
                .as_array()
                .into_iter()
                .flatten()
                .chain(incoming.as_array().ok_or("União exige lista")?)
            {
                if !a.contains(item) {
                    a.push(item.clone());
                }
            }
            value[k] = Value::Array(a);
        }
    }
    if let Some(options) = p.get("options") {
        let mut a = old["options"].as_array().cloned().unwrap_or_default();
        let mut seen = HashSet::new();
        for item in options.as_array().ok_or("Opções devem ser lista")? {
            let id = text(item, "id")?;
            if !seen.insert(id) {
                return Err("Opção duplicada no pacote".into());
            }
            if let Some(pos) = a.iter().position(|o| o["id"] == id) {
                a[pos] = merge(&a[pos], item)?;
            } else {
                a.push(item.clone());
            }
        }
        value["options"] = Value::Array(a);
    }
    value
        .as_object_mut()
        .ok_or("Registro inválido")?
        .entry("learning_status")
        .or_insert(json!("not_assessed"));
    value
        .as_object_mut()
        .ok_or("Registro inválido")?
        .entry("options_coverage")
        .or_insert(json!("selected"));
    validate_entry(&value)?;
    if let Some(a) = value["related_entries"].as_array() {
        for v in a {
            validate_id(v.as_str().ok_or("Relação inválida")?)?;
        }
    }
    Ok(value)
}
fn validate_asset_refs(conn: &Connection, b: &Value) -> StoreResult<()> {
    for k in ["asset_id", "preview_asset_id"] {
        if let Some(id) = b[k].as_str() {
            let mime: String =
                conn.query_row("SELECT media_type FROM assets WHERE id=?1", [id], |r| {
                    r.get(0)
                })?;
            if k == "preview_asset_id"
                && ![
                    "image/png",
                    "image/jpeg",
                    "image/svg+xml",
                    "application/pdf",
                ]
                .contains(&mime.as_str())
            {
                return Err("Formato de prévia incompatível".into());
            }
        }
    }
    Ok(())
}
fn rehearse(conn: &Connection, p: &Package) -> StoreResult<Preview> {
    validate_package_schema(p)?;
    let mut out = Preview {
        package_id: p.id.clone(),
        kind: p.kind.clone(),
        sha256: p.digest.clone(),
        ..Preview::default()
    };
    let previous: Option<(String, String)> = conn
        .query_row(
            "SELECT sha256,hash_format FROM import_history WHERE package_id=?1",
            [&p.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    if let Some((hash, format)) = previous {
        if hash != p.digest && (format != "raw" || hash != p.raw_digest) {
            return Err(
                "Conflito: package_id já usado com outro conteúdo; crie novo package_id".into(),
            );
        }
        out.already_applied = true;
        return Ok(out);
    }
    let mut sets = BTreeMap::new();
    let mut changes = BTreeMap::new();
    for kind in ["profiles", "areas", "modules"] {
        let mut data = indexed(conn, kind)?;
        let mut changed = BTreeSet::new();
        for patch in entity_list(&p.value, kind)? {
            let id = text(patch, "id")?;
            let old = data.get(id);
            let value = merge(old.unwrap_or(&json!({})), patch)?;
            hierarchy_entity(&value, kind)?;
            out.diff(&format!("{kind}/{id}"), old, &value);
            if old != Some(&value) {
                changed.insert(id.to_string());
            }
            data.insert(id.to_string(), value);
        }
        sets.insert(kind, data);
        changes.insert(kind, changed);
    }
    for kind in ["areas", "modules"] {
        for v in sets[kind].values() {
            if let Some(pid) = v["profile_id"].as_str() {
                if !sets["profiles"].contains_key(pid) {
                    return Err("Perfil referenciado não existe".into());
                }
            }
            if kind == "modules" && !sets["areas"].contains_key(text(v, "area_id")?) {
                return Err("Área de módulo inexistente".into());
            }
        }
    }
    let order = module_order(&sets["modules"])?;
    for id in &changes["profiles"] {
        let v = &sets["profiles"][id];
        conn.execute("INSERT INTO profiles(id,name,payload_json) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET name=excluded.name,payload_json=excluded.payload_json",params![id,text(v,"name")?,v.to_string()])?;
    }
    for id in &changes["areas"] {
        let v = &sets["areas"][id];
        conn.execute("INSERT INTO areas(id,name,profile_id,payload_json) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET name=excluded.name,profile_id=excluded.profile_id,payload_json=excluded.payload_json,revision=areas.revision+1",params![id,text(v,"name")?,v["profile_id"].as_str(),v.to_string()])?;
    }
    // Detach changed parents within the transaction so a valid edge reversal has no transient cycle.
    for id in &changes["modules"] {
        conn.execute("UPDATE modules SET parent_id=NULL WHERE id=?1", [id])?;
    }
    for id in order
        .into_iter()
        .filter(|id| changes["modules"].contains(id))
    {
        let v = &sets["modules"][&id];
        conn.execute("INSERT INTO modules(id,area_id,parent_id,name,profile_id,payload_json) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET area_id=excluded.area_id,parent_id=excluded.parent_id,name=excluded.name,profile_id=excluded.profile_id,payload_json=excluded.payload_json,revision=modules.revision+1",params![id,text(v,"area_id")?,v["parent_id"].as_str(),text(v,"name")?,v["profile_id"].as_str(),v.to_string()])?;
    }
    for (a, _) in &p.assets {
        let v = serde_json::to_value(a)?;
        let old=old_json(conn,"SELECT json_object('id',id,'filename',filename,'media_type',media_type,'sha256',sha256,'size',size) FROM assets WHERE id=?1",&a.id)?;
        if old.as_ref().is_some_and(|o| {
            o["sha256"] != v["sha256"]
                || o["media_type"] != v["media_type"]
                || o["size"] != v["size"]
        }) {
            return Err("Conflito: ID de anexo imutável; use novo ID".into());
        }
        out.diff(
            &format!("assets/{}", a.id),
            old.as_ref(),
            old.as_ref().unwrap_or(&v),
        );
        if old.is_none() {
            conn.execute(
                "INSERT INTO assets(id,filename,media_type,sha256,size) VALUES(?1,?2,?3,?4,?5)",
                params![a.id, a.filename, a.media_type, a.sha256, a.size],
            )?;
        }
    }
    for patch in entity_list(&p.value, "entries")? {
        let id = text(patch, "id")?;
        let old = old_json(conn, "SELECT payload_json FROM entries WHERE id=?1", id)?;
        let v = merge_entry(old.as_ref().unwrap_or(&json!({})), patch)?;
        out.diff(&format!("entries/{id}"), old.as_ref(), &v);
        let mut block_changed = false;
        let mut prepared = Vec::new();
        let mut pos: i64 = conn.query_row(
            "SELECT coalesce(max(position)+1,0) FROM blocks WHERE entry_id=?1",
            [id],
            |r| r.get(0),
        )?;
        for b in entity_list(patch, "blocks")? {
            let bid = text(b, "id")?;
            let existing: Option<(String, i64)> = conn
                .query_row(
                    "SELECT payload_json,position FROM blocks WHERE entry_id=?1 AND id=?2",
                    params![id, bid],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let old_block = existing
                .as_ref()
                .map(|(s, _)| serde_json::from_str::<Value>(s))
                .transpose()?;
            let merged = merge(old_block.as_ref().unwrap_or(&json!({})), b)?;
            validate_block(&merged)?;
            validate_asset_refs(conn, &merged)?;
            if old_block
                .as_ref()
                .is_some_and(|o| o["type"] != merged["type"])
            {
                return Err("Tipo de bloco é imutável".into());
            }
            out.diff(
                &format!("entries/{id}/blocks/{bid}"),
                old_block.as_ref(),
                &merged,
            );
            if old_block.as_ref() != Some(&merged) {
                block_changed = true;
                let position = existing.map(|(_, pos)| pos).unwrap_or_else(|| {
                    let p = pos;
                    pos += 1;
                    p
                });
                prepared.push((merged, position));
            }
        }
        if old.as_ref() != Some(&v) || block_changed {
            conn.execute("INSERT INTO entries(id,module_id,payload_json) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET module_id=excluded.module_id,payload_json=excluded.payload_json,revision=entries.revision+1",params![id,text(&v,"module_id")?,v.to_string()])?;
        }
        for (b, pos) in prepared {
            conn.execute("INSERT INTO blocks(entry_id,id,kind,position,payload_json) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(entry_id,id) DO UPDATE SET payload_json=excluded.payload_json",params![id,text(&b,"id")?,text(&b,"type")?,pos,b.to_string()])?;
        }
    }
    for key in ["app", "appearance", "preferences"] {
        if let Some(patch) = p.value.get(key) {
            let old = old_json(conn, "SELECT value_json FROM settings WHERE key=?1", key)?;
            let value = merge(old.as_ref().unwrap_or(&json!({})), patch)?;
            out.diff(&format!("settings/{key}"), old.as_ref(), &value);
            if old.as_ref() != Some(&value) {
                conn.execute("INSERT INTO settings(key,value_json) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json",params![key,value.to_string()])?;
            }
        }
    }
    conn.execute("INSERT INTO import_history(package_id,sha256,kind,hash_format) VALUES(?1,?2,?3,'canonical-v1')",params![p.id,p.digest,p.kind])?;
    Ok(out)
}
pub fn clone_conn(conn: &Connection) -> StoreResult<Connection> {
    let mut memory = Connection::open_in_memory()?;
    rusqlite::backup::Backup::new(conn, &mut memory)?.run_to_completion(
        128,
        Duration::from_millis(1),
        None,
    )?;
    memory.execute_batch("PRAGMA foreign_keys=ON;")?;
    Ok(memory)
}
pub fn state_hash(conn: &Connection) -> StoreResult<String> {
    crate::backup::logical_state(conn)
}
impl Store {
    pub fn preview_package(&self, input: &ImportInput) -> StoreResult<Preview> {
        let p = Package::read(&input.bytes, &input.format, &input.mode)?;
        self.preview_prepared(&p)
    }
    fn preview_prepared(&self, p: &Package) -> StoreResult<Preview> {
        let mut copy = clone_conn(&self.conn)?;
        let tx = copy.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch("PRAGMA defer_foreign_keys=ON;")?;
        let mut result = rehearse(&tx, p)?;
        tx.commit()?;
        result.review_token = hash(
            format!(
                "{}:{}:{}",
                p.digest,
                state_hash(&self.conn)?,
                canonical(&serde_json::to_value(&result.changes)?)
            )
            .as_bytes(),
        );
        Ok(result)
    }
    pub fn apply_package(
        &mut self,
        input: &ImportInput,
        approval: ImportApproval,
    ) -> StoreResult<Applied> {
        let package = Package::read(&input.bytes, &input.format, &input.mode)?;
        let expected_state = state_hash(&self.conn)?;
        let preview = self.preview_prepared(&package)?;
        if !approval.confirm || approval.review_token != preview.review_token {
            return Err("Confirme uma prévia atual; a base pode ter mudado".into());
        }
        let approved: HashSet<_> = approval.approved_changes.iter().collect();
        if preview
            .changes
            .iter()
            .any(|c| c.requires_approval && !approved.contains(&c.id))
        {
            return Err(
                "Confirmação específica ausente para conteúdo divergente, nota ou progresso".into(),
            );
        }
        if preview.already_applied {
            return Ok(Applied {
                preview,
                backup_id: None,
            });
        }
        let backup = self.create_backup()?;
        let staged = self.stage_assets(&package.assets)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch("PRAGMA defer_foreign_keys=ON;")?;
        if state_hash(&tx)? != expected_state {
            return Err("A base mudou durante o backup; revise a prévia novamente".into());
        }
        let result = (|| -> StoreResult<()> {
            rehearse(&tx, &package)?;
            staged.publish()?;
            tx.commit()?;
            Ok(())
        })();
        if let Err(e) = result {
            staged.rollback();
            return Err(e);
        }
        staged.finish();
        Ok(Applied {
            preview,
            backup_id: Some(backup.id),
        })
    }
    pub fn export_settings(&self) -> StoreResult<Value> {
        let mut v = json!({"schema_version":2,"type":"settings"});
        for kind in ["profiles", "areas", "modules"] {
            v[kind] =
                serde_json::to_value(indexed(&self.conn, kind)?.into_values().collect::<Vec<_>>())?;
        }
        for key in ["app", "appearance", "preferences"] {
            if let Some(value) = old_json(
                &self.conn,
                "SELECT value_json FROM settings WHERE key=?1",
                key,
            )? {
                v[key] = value;
            }
        }
        Ok(v)
    }
    pub fn export_record(&self, id: &str) -> StoreResult<Value> {
        validate_id(id)?;
        let mut entry = old_json(
            &self.conn,
            "SELECT payload_json FROM entries WHERE id=?1",
            id,
        )?
        .ok_or("Registro inexistente")?;
        let mut stmt = self
            .conn
            .prepare("SELECT payload_json FROM blocks WHERE entry_id=?1 ORDER BY position,id")?;
        let rows = stmt.query_map([id], |r| r.get::<_, String>(0))?;
        let blocks: Vec<Value> = rows
            .map(|r| Ok(serde_json::from_str(&r?)?))
            .collect::<StoreResult<_>>()?;
        entry["blocks"] = Value::Array(blocks);
        Ok(
            json!({"schema_version":2,"type":"knowledge_update","package_id":format!("export-{}",crate::backup::unique_id()),"entries":[entry]}),
        )
    }
}

pub fn validate_database(conn: &Connection) -> StoreResult<()> {
    for kind in ["profiles", "areas", "modules"] {
        for (id, value) in indexed(conn, kind)? {
            hierarchy_entity(&value, kind)?;
            if value["id"] != id {
                return Err("Identidade de hierarquia divergente".into());
            }
            let columns = match kind {
                "profiles" => vec!["name"],
                "areas" => vec!["name", "profile_id"],
                _ => vec!["name", "profile_id", "area_id", "parent_id"],
            };
            for column in columns {
                let actual: Option<String> = conn.query_row(
                    &format!("SELECT {column} FROM {kind} WHERE id=?1"),
                    [&id],
                    |r| r.get(0),
                )?;
                if actual.as_deref() != value[column].as_str() {
                    return Err("Payload e colunas da hierarquia divergem".into());
                }
            }
        }
    }
    module_order(&indexed(conn, "modules")?)?;
    let mut stmt = conn.prepare("SELECT id,payload_json FROM entries")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for row in rows {
        let (id, raw) = row?;
        let p: Value = package::strict_json(raw.as_bytes())?;
        let v = merge_entry(&p, &json!({}))?;
        if v["id"] != id {
            return Err("ID divergente no snapshot".into());
        }
    }
    let mut stmt = conn.prepare("SELECT id,kind,payload_json FROM blocks")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
        ))
    })?;
    for row in rows {
        let (id, kind, raw) = row?;
        let v = package::strict_json(raw.as_bytes())?;
        validate_block(&v)?;
        validate_asset_refs(conn, &v)?;
        if v["id"] != id || v["type"] != kind {
            return Err("Identidade de bloco divergente".into());
        }
    }
    Ok(())
}
