#[cfg(test)]
use crate::blocks::{validate_block, validate_entry};
use crate::{
    blocks::validate_id,
    storage::{Store, StoreResult},
};
use rusqlite::params;
#[cfg(test)]
use rusqlite::{OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(test)]
use std::collections::HashSet;
use std::{io::Write, path::Path};
const LIMIT: usize = 50 * 1024 * 1024;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetInput {
    pub id: String,
    pub filename: String,
    pub media_type: String,
    pub sha256: String,
    pub bytes: Vec<u8>,
}
#[derive(Serialize)]
pub struct Asset {
    pub id: String,
    pub filename: String,
    pub media_type: String,
    pub sha256: String,
    pub size: u64,
}
fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}
pub(crate) fn validate_asset(name: &str, mime: &str, bytes: &[u8]) -> StoreResult<()> {
    if bytes.is_empty() || bytes.len() > LIMIT {
        return Err("Anexo vazio ou maior que 50 MiB".into());
    }
    if name.is_empty()
        || name.len() > 240
        || name.contains(['/', '\\', ':'])
        || name.chars().any(char::is_control)
        || name == "."
        || name == ".."
    {
        return Err("Nome de anexo inválido".into());
    }
    let ext = Path::new(name)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match mime {
        "image/png" | "image/jpeg" => {
            let expected = if mime == "image/png" {
                image::ImageFormat::Png
            } else {
                image::ImageFormat::Jpeg
            };
            if image::guess_format(bytes)? != expected
                || !matches!(
                    (mime, ext.as_str()),
                    ("image/png", "png") | ("image/jpeg", "jpg" | "jpeg")
                )
            {
                return Err("Tipo real e extensão divergentes".into());
            }
            let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), expected);
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(12000);
            limits.max_image_height = Some(12000);
            limits.max_alloc = Some(128 * 1024 * 1024);
            reader.limits(limits);
            reader.decode()?;
        }
        "image/svg+xml" => {
            if ext != "svg" {
                return Err("Extensão SVG inválida".into());
            }
            let s = std::str::from_utf8(bytes)?;
            if s.contains("<!") {
                return Err("Declarações XML não permitidas".into());
            }
            let doc = roxmltree::Document::parse(s)?;
            if doc.root_element().tag_name().name() != "svg" {
                return Err("SVG inválido".into());
            }
            let tags = [
                "svg",
                "g",
                "path",
                "rect",
                "circle",
                "ellipse",
                "line",
                "polyline",
                "polygon",
                "text",
                "tspan",
                "title",
                "desc",
                "defs",
                "linearGradient",
                "radialGradient",
                "stop",
                "clipPath",
            ];
            let attrs = [
                "xmlns",
                "id",
                "viewBox",
                "width",
                "height",
                "x",
                "y",
                "x1",
                "x2",
                "y1",
                "y2",
                "cx",
                "cy",
                "r",
                "rx",
                "ry",
                "d",
                "points",
                "fill",
                "stroke",
                "stroke-width",
                "stroke-linecap",
                "stroke-linejoin",
                "stroke-dasharray",
                "opacity",
                "fill-opacity",
                "stroke-opacity",
                "transform",
                "font-size",
                "font-family",
                "font-weight",
                "text-anchor",
                "dominant-baseline",
                "offset",
                "stop-color",
                "stop-opacity",
                "gradientUnits",
                "gradientTransform",
                "clip-path",
                "preserveAspectRatio",
                "version",
            ];
            for n in doc.descendants().filter(|n| n.is_element()) {
                if !tags.contains(&n.tag_name().name())
                    || n.tag_name()
                        .namespace()
                        .is_some_and(|s| s != "http://www.w3.org/2000/svg")
                {
                    return Err("Elemento SVG ativo ou não permitido".into());
                }
                for a in n.attributes() {
                    let val = a.value().to_ascii_lowercase();
                    if !attrs.contains(&a.name())
                        || a.namespace().is_some()
                        || val.contains("url(")
                        || val.contains("http:")
                        || val.contains("https:")
                        || val.contains("data:")
                    {
                        return Err("Atributo SVG ativo ou recurso externo".into());
                    }
                }
            }
        }
        "application/pdf" => {
            if ext != "pdf"
                || !bytes.starts_with(b"%PDF-")
                || !bytes.windows(5).any(|w| w == b"%%EOF")
            {
                return Err("PDF inválido".into());
            }
        }
        "application/x-drawio" => {
            if ext != "drawio" {
                return Err("Extensão drawio inválida".into());
            }
            let s = std::str::from_utf8(bytes)?;
            let doc = roxmltree::Document::parse(s)?;
            if !["mxfile", "mxGraphModel"].contains(&doc.root_element().tag_name().name()) {
                return Err("Original drawio XML inválido".into());
            }
        }
        // Packet Tracer is proprietary: never claim to parse or render its opaque payload.
        "application/octet-stream" => {
            if ext != "pkt" {
                return Err(
                    "Binário opaco permitido somente como original .pkt, sem visualização".into(),
                );
            }
        }
        _ => return Err("Tipo de anexo não suportado".into()),
    }
    Ok(())
}
impl Store {
    fn asset_root(&self) -> StoreResult<std::path::PathBuf> {
        if self.path == ":memory:" {
            return Err("Anexos exigem armazenamento persistente".into());
        }
        Ok(Path::new(&self.path)
            .parent()
            .ok_or("Diretório de dados indisponível")?
            .join("assets"))
    }
    pub fn asset(&self, id: &str) -> StoreResult<Asset> {
        validate_id(id)?;
        Ok(self.conn.query_row(
            "SELECT id,filename,media_type,sha256,size FROM assets WHERE id=?1",
            [id],
            |r| {
                Ok(Asset {
                    id: r.get(0)?,
                    filename: r.get(1)?,
                    media_type: r.get(2)?,
                    sha256: r.get(3)?,
                    size: r.get(4)?,
                })
            },
        )?)
    }
    pub fn read_asset(&self, id: &str) -> StoreResult<Vec<u8>> {
        let a = self.asset(id)?;
        let path = self.asset_root()?.join(format!("{}-{}", a.id, a.sha256));
        let meta = std::fs::symlink_metadata(&path)?;
        if !meta.is_file()
            || meta.file_type().is_symlink()
            || meta.len() != a.size
            || meta.len() > LIMIT as u64
        {
            return Err("Arquivo gerenciado inválido".into());
        }
        let bytes = std::fs::read(path)?;
        if hash(&bytes) != a.sha256 {
            return Err("Integridade do anexo divergente".into());
        }
        validate_asset(&a.filename, &a.media_type, &bytes)?;
        Ok(bytes)
    }
    pub fn put_asset(&mut self, input: AssetInput) -> StoreResult<Asset> {
        validate_id(&input.id)?;
        validate_asset(&input.filename, &input.media_type, &input.bytes)?;
        if hash(&input.bytes) != input.sha256 {
            return Err("SHA-256 divergente".into());
        }
        if let Ok(a) = self.asset(&input.id) {
            if a.sha256 == input.sha256
                && a.filename == input.filename
                && a.media_type == input.media_type
            {
                self.read_asset(&input.id)?;
                return Ok(a);
            }
            return Err("ID de anexo imutável; use um novo ID".into());
        }
        let root = self.asset_root()?;
        std::fs::create_dir_all(&root)?;
        if std::fs::symlink_metadata(&root)?.file_type().is_symlink() {
            return Err("Armazenamento de anexos não pode ser link".into());
        }
        let destination = root.join(format!("{}-{}", input.id, input.sha256));
        let stage = root.join(format!(".staging-{}", input.id));
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)?;
        let result = (|| -> StoreResult<()> {
            f.write_all(&input.bytes)?;
            f.sync_all()?;
            drop(f);
            if destination.exists() {
                return Err("Arquivo final já existe sem metadados; verificar integridade antes de tentar novamente".into());
            }
            std::fs::rename(&stage, &destination)?;
            if let Err(e) = self.conn.execute(
                "INSERT INTO assets(id,filename,media_type,sha256,size) VALUES(?1,?2,?3,?4,?5)",
                params![
                    input.id,
                    input.filename,
                    input.media_type,
                    input.sha256,
                    input.bytes.len() as i64
                ],
            ) {
                let _ = std::fs::remove_file(&destination);
                return Err(e.into());
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(stage);
        }
        result?;
        self.asset(&input.id)
    }
    /// Narrow stage-4 importer: only new entries; existing records remain untouched.
    #[cfg(test)]
    pub fn import_visual(&mut self, raw: &str, confirm: bool) -> StoreResult<Value> {
        if raw.len() > 8 * 1024 * 1024 {
            return Err("JSON excede 8 MiB".into());
        }
        let package: Value = serde_json::from_str(raw)?;
        if package["schema_version"] != 2 || package["type"] != "knowledge_update" {
            return Err("Esperado knowledge_update v2".into());
        }
        for k in package.as_object().ok_or("Objeto obrigatório")?.keys() {
            if !["schema_version", "type", "package_id", "entries"].contains(&k.as_str()) {
                return Err("Campo de pacote desconhecido".into());
            }
        }
        let pid = package["package_id"]
            .as_str()
            .ok_or("package_id obrigatório")?;
        validate_id(pid)?;
        let digest = hash(raw.as_bytes());
        let old: Option<String> = self
            .conn
            .query_row(
                "SELECT sha256 FROM import_history WHERE package_id=?1",
                [pid],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(old) = old {
            if old != digest {
                return Err("Pacote alterado sem novo ID".into());
            }
            return Ok(json!({"already_applied":true,"entries":0}));
        }
        let entries = package["entries"].as_array().ok_or("entries obrigatório")?;
        if entries.is_empty() || entries.len() > 1000 {
            return Err("Quantidade de registros inválida".into());
        }
        let mut prepared = Vec::new();
        let mut ids = HashSet::new();
        for e in entries {
            let mut payload = e.clone();
            let obj = payload.as_object_mut().ok_or("Registro inválido")?;
            let blocks = obj.remove("blocks").unwrap_or(json!([]));
            obj.entry("learning_status")
                .or_insert(json!("not_assessed"));
            obj.entry("options_coverage").or_insert(json!("selected"));
            validate_entry(&payload)?;
            let id = payload["id"].as_str().ok_or("id obrigatório")?;
            if !ids.insert(id.to_string())
                || self
                    .conn
                    .query_row("SELECT count(*) FROM entries WHERE id=?1", [id], |r| {
                        r.get::<_, i64>(0)
                    })?
                    > 0
            {
                return Err("Esta etapa importa somente IDs novos; atualização incremental disponível na etapa 5".into());
            }
            let mid = payload["module_id"]
                .as_str()
                .ok_or("module_id obrigatório")?;
            if self
                .conn
                .query_row("SELECT count(*) FROM modules WHERE id=?1", [mid], |r| {
                    r.get::<_, i64>(0)
                })?
                != 1
            {
                return Err("Importe/crie o módulo e seu perfil antes do material visual".into());
            }
            let mut bids = HashSet::new();
            for b in blocks.as_array().ok_or("blocks deve ser lista")? {
                validate_block(b)?;
                if !bids.insert(b["id"].as_str().ok_or("Bloco sem ID")?) {
                    return Err("ID de bloco duplicado".into());
                }
                if let Some(a) = b["asset_id"].as_str() {
                    self.asset(a)?;
                }
                if let Some(a) = b["preview_asset_id"].as_str() {
                    let preview = self.asset(a)?;
                    if ![
                        "image/png",
                        "image/jpeg",
                        "image/svg+xml",
                        "application/pdf",
                    ]
                    .contains(&preview.media_type.as_str())
                    {
                        return Err("Formato de prévia incompatível".into());
                    }
                }
            }
            prepared.push((payload, blocks));
        }
        if !confirm {
            return Ok(
                json!({"already_applied":false,"entries":prepared.len(),"titles":prepared.iter().map(|(p,_)|p["title"].clone()).collect::<Vec<_>>(),"sha256":digest}),
            );
        }
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (p, blocks) in &prepared {
            tx.execute(
                "INSERT INTO entries(id,module_id,payload_json) VALUES(?1,?2,?3)",
                params![p["id"].as_str(), p["module_id"].as_str(), p.to_string()],
            )?;
            for (i, b) in blocks
                .as_array()
                .ok_or("Blocos inválidos")?
                .iter()
                .enumerate()
            {
                tx.execute("INSERT INTO blocks(entry_id,id,kind,position,payload_json) VALUES(?1,?2,?3,?4,?5)",params![p["id"].as_str(),b["id"].as_str(),b["type"].as_str(),i as i64,b.to_string()])?;
            }
        }
        tx.execute(
            "INSERT INTO import_history(package_id,sha256,kind) VALUES(?1,?2,'knowledge_update')",
            params![pid, digest],
        )?;
        tx.commit()?;
        Ok(json!({"already_applied":false,"entries":prepared.len()}))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SearchRequest;
    const FIXTURE: &str = include_str!("../../examples/topologia.json");
    #[test]
    fn fixture_import_search_and_atomic_rejection() -> StoreResult<()> {
        let mut db = Store::memory()?;
        let profiles: i64 =
            db.conn
                .query_row("SELECT count(*) FROM profiles WHERE id='redes'", [], |r| {
                    r.get(0)
                })?;
        assert_eq!(profiles, 1);
        let module: i64 = db.conn.query_row(
            "SELECT count(*) FROM modules WHERE id='redes-topologias'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(module, 1);
        assert_eq!(db.import_visual(FIXTURE, false)?["entries"], 1);
        assert!(db.entry("redes-estrela-demo").is_err());
        db.import_visual(FIXTURE, true)?;
        let e = db.entry("redes-estrela-demo")?;
        assert_eq!(e.blocks.iter().filter(|b| !b.virtual_block).count(), 4);
        assert_eq!(
            e.blocks
                .iter()
                .find(|b| b.payload["type"] == "topology")
                .unwrap()
                .payload["edges"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        for query in ["Ethernet UTP", "switch", "ligação de computadores"] {
            let found = db.search(SearchRequest {
                query: query.into(),
                limit: None,
                filters: None,
            })?;
            assert!(
                found
                    .iter()
                    .any(|r| r.id == "redes-estrela-demo" && r.visual),
                "{query}"
            );
        }
        assert_eq!(db.import_visual(FIXTURE, true)?["already_applied"], true);
        for bad in [
            FIXTURE.replace(
                "\"pc1\",\n          \"from_port\"",
                "\"missing\",\n          \"from_port\"",
            ),
            FIXTURE.replace("\"topology\"", "\"alien\""),
        ] {
            let mut p: Value = serde_json::from_str(&bad)?;
            p["package_id"] = json!("bad-package");
            p["entries"][0]["id"] = json!("bad-entry");
            // Change endpoints structurally, independent of fixture whitespace.
            if p["entries"][0]["blocks"][0]["type"] == "topology" {
                p["entries"][0]["blocks"][0]["edges"][0]["to"] = json!("missing");
            }
            assert!(db.import_visual(&p.to_string(), true).is_err());
            assert!(db.entry("bad-entry").is_err());
        }
        Ok(())
    }
    #[test]
    fn svg_rejects_active_content_and_mismatched_types() -> StoreResult<()> {
        let safe=br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><rect width="10" height="10" fill="#fff"/></svg>"##;
        validate_asset("safe.svg", "image/svg+xml", safe)?;
        for bad in [
            "<svg><script>alert(1)</script></svg>",
            "<svg onload='x'/>",
            "<svg><image href='https://evil'/></svg>",
            "<!DOCTYPE svg><svg/>",
            "<svg><a href='x'/></svg>",
            "<svg><rect style='fill:url(x)'/></svg>",
        ] {
            assert!(validate_asset("bad.svg", "image/svg+xml", bad.as_bytes()).is_err());
        }
        assert!(validate_asset("x.png", "image/png", safe).is_err());
        assert!(validate_asset("../x.svg", "image/svg+xml", safe).is_err());
        Ok(())
    }
    #[test]
    fn actual_image_formats_and_originals_are_checked() -> StoreResult<()> {
        let png = include_bytes!("../icons/sol.png");
        validate_asset("logo.png", "image/png", png)?;
        assert!(validate_asset("logo.jpg", "image/jpeg", png).is_err());
        let mut jpg = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(2, 2).write_to(&mut jpg, image::ImageFormat::Jpeg)?;
        validate_asset("test.jpg", "image/jpeg", jpg.get_ref())?;
        assert!(validate_asset("broken.jpg", "image/jpeg", &jpg.get_ref()[..8]).is_err());
        validate_asset(
            "original.drawio",
            "application/x-drawio",
            b"<mxfile><diagram>opaque compressed data</diagram></mxfile>",
        )?;
        assert!(validate_asset("fake.drawio", "application/x-drawio", b"<html/>").is_err());
        assert!(validate_asset("fake.pdf", "application/pdf", b"not pdf").is_err());
        Ok(())
    }
    #[test]
    fn managed_assets_hash_id_reopen_and_filename_search() -> StoreResult<()> {
        let root = std::env::temp_dir().join(format!(
            "sol-visual-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir_all(&root)?;
        let path = root.join("knowledge.sqlite3");
        let mut db = Store::open(&path)?;
        let bytes =
            br##"<svg xmlns="http://www.w3.org/2000/svg"><rect width="10" height="10"/></svg>"##
                .to_vec();
        let make = || AssetInput {
            id: "asset-fixture".into(),
            filename: "cabeamento-especial.svg".into(),
            media_type: "image/svg+xml".into(),
            sha256: hash(&bytes),
            bytes: bytes.clone(),
        };
        let mut wrong = make();
        wrong.sha256 = "0".repeat(64);
        assert!(db.put_asset(wrong).is_err());
        db.put_asset(make())?;
        db.put_asset(make())?;
        db.import_visual(FIXTURE, true)?;
        let e = db.entry("redes-estrela-demo")?;
        db.save_block(crate::models::BlockInput {
            entry_id: e.id,
            expected_revision: e.revision,
            patch: json!({"id":"preview","type":"image","asset_id":"asset-fixture"}),
            confirm_personal: false,
        })?;
        assert!(db
            .search(SearchRequest {
                query: "cabeamento especial".into(),
                limit: None,
                filters: None
            })?
            .iter()
            .any(|r| r.id == "redes-estrela-demo"));
        let original = b"<mxfile/>".to_vec();
        db.put_asset(AssetInput {
            id: "original-fixture".into(),
            filename: "original.drawio".into(),
            media_type: "application/x-drawio".into(),
            sha256: hash(&original),
            bytes: original,
        })?;
        let entry = db.entry("redes-estrela-demo")?;
        let detail=db.save_block(crate::models::BlockInput{entry_id:entry.id,expected_revision:entry.revision,patch:json!({"id":"original","type":"attachment","asset_id":"original-fixture","preview_asset_id":"asset-fixture"}),confirm_personal:false})?;
        assert!(db
            .save_block(crate::models::BlockInput {
                entry_id: detail.id,
                expected_revision: detail.revision,
                patch: json!({"id":"original","preview_asset_id":"original-fixture"}),
                confirm_personal: false
            })
            .is_err());
        drop(db);
        let db = Store::open(&path)?;
        assert_eq!(db.read_asset("asset-fixture")?, bytes);
        std::fs::write(
            root.join("assets")
                .join(format!("asset-fixture-{}", hash(&bytes))),
            b"tampered",
        )?;
        assert!(db.read_asset("asset-fixture").is_err());
        drop(db);
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
}
