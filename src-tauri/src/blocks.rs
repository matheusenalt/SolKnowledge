use crate::{
    models::*,
    storage::{Store, StoreResult},
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde_json::{json, Value};
use std::collections::HashSet;

pub const COMPONENTS: &[&str] = &[
    "text",
    "personal_note",
    "command",
    "code",
    "warning",
    "reference",
    "diagram",
    "topology",
    "image",
    "attachment",
    "equipment",
    "cabling",
    "procedure",
    "troubleshooting",
    "table",
];
pub const ENTRY_TYPES: &[&str] = &[
    "command",
    "concept",
    "summary",
    "procedure",
    "troubleshooting",
    "lab",
    "project",
    "note",
    "reference",
];
pub const STATUSES: &[&str] = &["not_assessed", "studying", "understood", "applied"];

pub fn validate_id(id: &str) -> StoreResult<()> {
    if id.len() > 128
        || !id.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
    {
        return Err("ID inválido".into());
    }
    Ok(())
}
pub fn validate_components(list: &[String]) -> StoreResult<()> {
    let mut seen = HashSet::new();
    for c in list {
        if !COMPONENTS.contains(&c.as_str()) || !seen.insert(c) {
            return Err("Componente desconhecido ou duplicado".into());
        }
    }
    Ok(())
}
fn object_keys(value: &Value, allowed: &[&str]) -> StoreResult<()> {
    let o = value.as_object().ok_or("Objeto JSON obrigatório")?;
    if value.to_string().len() > 1024 * 1024 {
        return Err("Conteúdo maior que 1 MiB".into());
    }
    for k in o.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(format!("Campo desconhecido: {k}").into());
        }
    }
    Ok(())
}
fn required_text<'a>(v: &'a Value, key: &str) -> StoreResult<&'a str> {
    v[key]
        .as_str()
        .ok_or_else(|| format!("Texto obrigatório: {key}").into())
}
fn string_array(v: &Value) -> StoreResult<()> {
    let a = v.as_array().ok_or("Lista de textos obrigatória")?;
    if a.len() > 2000 || a.iter().any(|v| !v.is_string()) {
        return Err("Lista de textos inválida".into());
    }
    Ok(())
}
fn merge(old: &Value, patch: &Value) -> StoreResult<Value> {
    let mut value = old.as_object().cloned().ok_or("Objeto inválido")?;
    for (key, item) in patch.as_object().ok_or("Patch deve ser objeto")? {
        if item.is_null() {
            return Err("Omitir preserva; null não é permitido neste editor".into());
        }
        value.insert(key.clone(), item.clone());
    }
    Ok(Value::Object(value))
}
fn confirm_personal(old: &Value, new: &Value, key: &str, confirmed: bool) -> StoreResult<()> {
    if old[key].as_str().is_some_and(|s| !s.is_empty()) && old[key] != new[key] && !confirmed {
        return Err("Confirmação específica necessária para alterar o texto pessoal".into());
    }
    Ok(())
}

pub fn legacy_blocks(v: &Value) -> Vec<BlockView> {
    let mut result = Vec::new();
    for (field, kind, title) in [
        ("technical_description", "text", "Explicação técnica"),
        ("simplified_description", "text", "Em outras palavras"),
        ("personal_description", "personal_note", "Meu entendimento"),
        ("markdown", "text", "Anotações"),
    ] {
        if let Some(s) = v[field].as_str().filter(|s| !s.is_empty()) {
            result.push(BlockView {
                virtual_block: true,
                payload: json!({"id":format!("v1:{field}"),"type":kind,"title":title,"content":s}),
            });
        }
    }
    if v["type"] == "command" || v["options"].as_array().is_some_and(|a| !a.is_empty()) {
        result.push(BlockView{virtual_block:true,payload:json!({"id":"v1:command","type":"command","title":"Comando e opções","content":v["title"],"options":v["options"],"coverage":v["options_coverage"],"risk":v["risk"]})});
    }
    if let Some(sources) = v["sources"].as_array().filter(|a| !a.is_empty()) {
        result.push(BlockView{virtual_block:true,payload:json!({"id":"v1:sources","type":"reference","title":"Fontes","content":sources.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n")})});
    }
    result
}

pub(crate) fn validate_entry(v: &Value) -> StoreResult<()> {
    object_keys(
        v,
        &[
            "id",
            "module_id",
            "type",
            "title",
            "full_name",
            "technical_description",
            "simplified_description",
            "personal_description",
            "keywords",
            "related_entries",
            "options",
            "options_coverage",
            "learning_status",
            "sources",
            "markdown",
            "risk",
            "platform",
        ],
    )?;
    validate_id(required_text(v, "id")?)?;
    validate_id(required_text(v, "module_id")?)?;
    if required_text(v, "title")?.trim().is_empty() {
        return Err("Título obrigatório".into());
    }
    if !ENTRY_TYPES.contains(&required_text(v, "type")?)
        || !STATUSES.contains(&required_text(v, "learning_status")?)
    {
        return Err("Tipo/estado desconhecido".into());
    }
    for k in [
        "title",
        "full_name",
        "technical_description",
        "simplified_description",
        "personal_description",
        "markdown",
        "platform",
    ] {
        if let Some(s) = v.get(k) {
            if !s.is_string() {
                return Err(format!("{k} deve ser texto").into());
            }
        }
    }
    for k in ["keywords", "related_entries", "sources"] {
        if let Some(s) = v.get(k) {
            string_array(s)?;
        }
    }
    if let Some(r) = v.get("risk") {
        if ![
            "read_only",
            "changes_state",
            "destructive",
            "context_dependent",
        ]
        .contains(&r.as_str().ok_or("Risco inválido")?)
        {
            return Err("Risco inválido".into());
        }
    }
    match required_text(v, "options_coverage")? {
        "selected" => {}
        "complete"
            if v["platform"].as_str().is_some_and(|s| !s.trim().is_empty())
                && v["sources"].as_array().is_some_and(|a| !a.is_empty()) => {}
        _ => return Err("Cobertura completa exige versão/plataforma e fontes".into()),
    }
    if let Some(options) = v.get("options") {
        let mut ids = HashSet::new();
        for option in options.as_array().ok_or("Opções devem ser uma lista")? {
            object_keys(
                option,
                &[
                    "id",
                    "example",
                    "nomenclature",
                    "description",
                    "application",
                    "featured",
                ],
            )?;
            let id = required_text(option, "id")?;
            validate_id(id)?;
            if !ids.insert(id) {
                return Err("Opção duplicada".into());
            }
            required_text(option, "example")?;
            for k in ["nomenclature", "description", "application"] {
                if option.get(k).is_some_and(|s| !s.is_string()) {
                    return Err("Campo de opção deve ser texto".into());
                }
            }
            if option.get("featured").is_some_and(|v| !v.is_boolean()) {
                return Err("featured deve ser booleano".into());
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_block(v: &Value) -> StoreResult<()> {
    object_keys(
        v,
        &[
            "id",
            "type",
            "title",
            "content",
            "format",
            "language",
            "asset_id",
            "caption",
            "keywords",
            "items",
            "columns",
            "rows",
            "nodes",
            "edges",
            "preview_asset_id",
        ],
    )?;
    validate_id(required_text(v, "id")?)?;
    let kind = required_text(v, "type")?;
    if let Some(preview) = v.get("preview_asset_id") {
        if kind != "attachment" {
            return Err("Prévia associada exige bloco de anexo".into());
        }
        validate_id(preview.as_str().ok_or("ID de prévia inválido")?)?;
    }
    if !COMPONENTS.contains(&kind) {
        return Err("Tipo de bloco desconhecido".into());
    }
    for k in ["title", "content", "language", "caption", "asset_id"] {
        if v.get(k).is_some_and(|s| !s.is_string()) {
            return Err(format!("{k} deve ser texto").into());
        }
    }
    for k in ["keywords", "items", "columns"] {
        if let Some(a) = v.get(k) {
            string_array(a)?;
        }
    }
    if let Some(f) = v.get("format") {
        if !["markdown", "plain", "mermaid", "asset", "graph"]
            .contains(&f.as_str().ok_or("Formato inválido")?)
        {
            return Err("Formato inválido".into());
        }
    }
    match kind {
        "text" | "personal_note" | "warning" | "reference" | "command" => {
            required_text(v, "content")?;
        }
        "code" => {
            required_text(v, "content")?;
            required_text(v, "language")?;
        }
        "diagram" if v["format"] == "mermaid" => {
            required_text(v, "content")?;
        }
        "diagram" if v["format"] == "asset" => {
            validate_id(required_text(v, "asset_id")?)?;
        }
        "image" | "attachment" => {
            validate_id(required_text(v, "asset_id")?)?;
        }
        "equipment" | "cabling" | "procedure" | "troubleshooting" => {
            if v.get("items").is_none() {
                required_text(v, "content")?;
            }
        }
        "table" => {
            string_array(&v["columns"])?;
            let width = v["columns"].as_array().ok_or("Colunas inválidas")?.len();
            for row in v["rows"].as_array().ok_or("Linhas inválidas")? {
                string_array(row)?;
                if row.as_array().ok_or("Linha inválida")?.len() != width {
                    return Err("Largura da tabela divergente".into());
                }
            }
        }
        "topology" if v["format"] == "graph" => {
            if v["nodes"].as_array().is_some_and(|a| a.len() > 500)
                || v["edges"].as_array().is_some_and(|a| a.len() > 1000)
            {
                return Err("Topologia excede 500 nós ou 1000 enlaces".into());
            }
            let mut nodes = HashSet::new();
            for n in v["nodes"].as_array().ok_or("Nós obrigatórios")? {
                object_keys(n, &["id", "label", "kind", "ip", "notes"])?;
                let id = required_text(n, "id")?;
                validate_id(id)?;
                required_text(n, "label")?;
                if !nodes.insert(id) {
                    return Err("Nó duplicado".into());
                }
                for k in ["kind", "ip", "notes"] {
                    if n.get(k).is_some_and(|s| !s.is_string()) {
                        return Err("Texto de nó inválido".into());
                    }
                }
            }
            let mut edges = HashSet::new();
            for edge in v["edges"].as_array().ok_or("Enlaces obrigatórios")? {
                object_keys(
                    edge,
                    &[
                        "id",
                        "from",
                        "to",
                        "from_port",
                        "to_port",
                        "medium",
                        "label",
                    ],
                )?;
                let id = required_text(edge, "id")?;
                validate_id(id)?;
                if !edges.insert(id)
                    || !nodes.contains(required_text(edge, "from")?)
                    || !nodes.contains(required_text(edge, "to")?)
                {
                    return Err("Enlace duplicado ou extremo ausente".into());
                }
                for k in ["from_port", "to_port", "medium", "label"] {
                    if edge.get(k).is_some_and(|s| !s.is_string()) {
                        return Err("Texto de enlace inválido".into());
                    }
                }
            }
        }
        _ => return Err("Payload incompatível com o tipo de bloco".into()),
    }
    Ok(())
}

impl Store {
    pub fn save_entry(&mut self, input: EntryInput) -> StoreResult<EntryDetail> {
        validate_id(&input.id)?;
        object_keys(
            &input.patch,
            &[
                "module_id",
                "type",
                "title",
                "full_name",
                "technical_description",
                "simplified_description",
                "personal_description",
                "keywords",
                "related_entries",
                "options",
                "options_coverage",
                "learning_status",
                "sources",
                "markdown",
                "risk",
                "platform",
            ],
        )?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: Option<(String, i64)> = tx
            .query_row(
                "SELECT payload_json,revision FROM entries WHERE id=?1",
                [&input.id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (mut base, revision) = match old {
            Some((raw, revision)) if Some(revision) == input.expected_revision => {
                (serde_json::from_str::<Value>(&raw)?, revision)
            }
            None if input.expected_revision.is_none() => (
                json!({"id":input.id,"learning_status":"not_assessed","options_coverage":"selected"}),
                0,
            ),
            _ => {
                return Err("Registro alterado ou ID já existe; recarregue antes de salvar".into())
            }
        };
        // Old v1 payloads may omit fields whose SQL projections already have defaults.
        let object = base.as_object_mut().ok_or("Registro inválido")?;
        object
            .entry("learning_status")
            .or_insert(json!("not_assessed"));
        object
            .entry("options_coverage")
            .or_insert(json!("selected"));
        let value = merge(&base, &input.patch)?;
        validate_entry(&value)?;
        confirm_personal(
            &base,
            &value,
            "personal_description",
            input.confirm_personal,
        )?;
        tx.execute("INSERT INTO entries(id,module_id,payload_json,revision) VALUES(?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET module_id=excluded.module_id,payload_json=excluded.payload_json,revision=excluded.revision",params![input.id,required_text(&value,"module_id")?,value.to_string(),revision+1])?;
        tx.commit()?;
        self.entry(&input.id)
    }
    pub fn save_block(&mut self, input: BlockInput) -> StoreResult<EntryDetail> {
        let patch = &input.patch;
        let id = required_text(patch, "id")?;
        validate_id(id)?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let revision: i64 = tx.query_row(
            "SELECT revision FROM entries WHERE id=?1",
            [&input.entry_id],
            |r| r.get(0),
        )?;
        if revision != input.expected_revision {
            return Err("Registro alterado; recarregue".into());
        }
        let old: Option<(String, i64)> = tx
            .query_row(
                "SELECT payload_json,position FROM blocks WHERE entry_id=?1 AND id=?2",
                params![input.entry_id, id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let (base, position) = match old {
            Some((raw, pos)) => (serde_json::from_str::<Value>(&raw)?, pos),
            None => (
                json!({"id":id}),
                tx.query_row(
                    "SELECT coalesce(max(position),-1)+1 FROM blocks WHERE entry_id=?1",
                    [&input.entry_id],
                    |r| r.get(0),
                )?,
            ),
        };
        let value = merge(&base, patch)?;
        validate_block(&value)?;
        if base.get("type").is_some() && base["type"] != value["type"] {
            return Err("O tipo do bloco é imutável".into());
        }
        if value["type"] == "personal_note" {
            confirm_personal(&base, &value, "content", input.confirm_personal)?;
        }
        if let Some(asset) = value["asset_id"].as_str() {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1)",
                [asset],
                |r| r.get(0),
            )?;
            if !exists {
                return Err("Anexo não cadastrado".into());
            }
        }
        if let Some(preview) = value["preview_asset_id"].as_str() {
            let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND media_type IN ('image/png','image/jpeg','image/svg+xml','application/pdf'))",[preview],|r|r.get(0))?;
            if !valid {
                return Err("Prévia não cadastrada ou formato incompatível".into());
            }
        }
        tx.execute("INSERT INTO blocks(entry_id,id,kind,position,payload_json) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(entry_id,id) DO UPDATE SET payload_json=excluded.payload_json",params![input.entry_id,id,required_text(&value,"type")?,position,value.to_string()])?;
        tx.execute(
            "UPDATE entries SET revision=revision+1 WHERE id=?1",
            [&input.entry_id],
        )?;
        tx.commit()?;
        self.entry(&input.entry_id)
    }
    pub fn reorder_blocks(&mut self, input: BlockOrder) -> StoreResult<EntryDetail> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let revision: i64 = tx.query_row(
            "SELECT revision FROM entries WHERE id=?1",
            [&input.entry_id],
            |r| r.get(0),
        )?;
        if revision != input.expected_revision {
            return Err("Registro alterado; recarregue".into());
        }
        let current: HashSet<String> = {
            let mut stmt = tx.prepare("SELECT id FROM blocks WHERE entry_id=?1")?;
            let rows = stmt.query_map([&input.entry_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let new: HashSet<_> = input.ids.iter().cloned().collect();
        if new != current || new.len() != input.ids.len() {
            return Err("Reordenação precisa conter cada bloco exatamente uma vez".into());
        }
        for (pos, id) in input.ids.iter().enumerate() {
            tx.execute(
                "UPDATE blocks SET position=?1 WHERE entry_id=?2 AND id=?3",
                params![pos as i64, input.entry_id, id],
            )?;
        }
        tx.execute(
            "UPDATE entries SET revision=revision+1 WHERE id=?1",
            [&input.entry_id],
        )?;
        tx.commit()?;
        self.entry(&input.entry_id)
    }
}
