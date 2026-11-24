//! Strict JSON, deterministic hashes and bounded archive transport. No input paths reach disk.
use crate::{
    storage::StoreResult,
    visual::{validate_asset, Asset},
};
use serde::{
    de::{self, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fmt,
    io::{Cursor, Read, Write},
};
pub const FILE_LIMIT: u64 = 50 * 1024 * 1024;
pub const TOTAL_LIMIT: u64 = 250 * 1024 * 1024;
pub const JSON_LIMIT: usize = 8 * 1024 * 1024;
pub fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}
struct Strict(Value);
impl<'de> Deserialize<'de> for Strict {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Strict;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON estrito sem chaves duplicadas")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Strict, E> {
                Ok(Strict(Value::Number(
                    Number::from_f64(v).ok_or_else(|| E::custom("Número inválido"))?,
                )))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Strict, E> {
                Ok(Strict(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Strict, E> {
                Ok(Strict(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Strict, A::Error> {
                let mut list = Vec::new();
                while let Some(v) = seq.next_element::<Strict>()? {
                    list.push(v.0);
                }
                Ok(Strict(Value::Array(list)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Strict, A::Error> {
                let mut result = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if result.contains_key(&key) {
                        return Err(de::Error::custom(format!("Chave JSON duplicada: {key}")));
                    }
                    let v = map.next_value::<Strict>()?;
                    result.insert(key, v.0);
                }
                Ok(Strict(Value::Object(result)))
            }
        }
        d.deserialize_any(V)
    }
}
pub fn strict_json(bytes: &[u8]) -> StoreResult<Value> {
    if bytes.len() > JSON_LIMIT {
        return Err("JSON excede 8 MiB".into());
    }
    let mut d = serde_json::Deserializer::from_slice(bytes);
    let v = Strict::deserialize(&mut d)?.0;
    d.end()?;
    Ok(v)
}
pub fn canonical(v: &Value) -> String {
    match v {
        Value::Object(m) => {
            let ordered: BTreeMap<_, _> = m.iter().collect();
            format!(
                "{{{}}}",
                ordered
                    .into_iter()
                    .map(|(k, v)| format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap_or_default(),
                        canonical(v)
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        _ => v.to_string(),
    }
}
pub fn keys(v: &Value, allowed: &[&str]) -> StoreResult<()> {
    for k in v.as_object().ok_or("Objeto JSON obrigatório")?.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(format!("Campo desconhecido: {k}").into());
        }
    }
    Ok(())
}
pub fn text<'a>(v: &'a Value, k: &str) -> StoreResult<&'a str> {
    v[k].as_str()
        .ok_or_else(|| format!("Texto obrigatório: {k}").into())
}
pub fn list<'a>(v: &'a Value, k: &str) -> StoreResult<&'a [Value]> {
    match v.get(k) {
        None => Ok(&[]),
        Some(Value::Array(a)) if a.len() <= 10000 => Ok(a),
        _ => Err(format!("Lista inválida: {k}").into()),
    }
}
pub fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 240
        && !name.contains(['/', '\\', ':'])
        && !name.chars().any(char::is_control)
        && name != "."
        && name != ".."
        && !name.ends_with(['.', ' '])
}
/// Read bounded bytes without extracting archive names. Even directory entries are allowlisted.
pub fn read_zip(bytes: &[u8]) -> StoreResult<BTreeMap<String, Vec<u8>>> {
    if bytes.len() as u64 > TOTAL_LIMIT {
        return Err("ZIP comprimido excede 250 MiB".into());
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    if archive.len() > 1000 {
        return Err("ZIP excede 1000 membros".into());
    }
    let mut declared_total = 0u64;
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        if file.size() > FILE_LIMIT {
            return Err("Arquivo ZIP excede 50 MiB".into());
        }
        declared_total = declared_total
            .checked_add(file.size())
            .ok_or("Tamanho ZIP inválido")?;
        if declared_total > TOTAL_LIMIT {
            return Err("ZIP descompactado excede 250 MiB".into());
        }
    }
    let mut files = BTreeMap::new();
    let mut seen = HashSet::new();
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut f = archive.by_index(i)?;
        let name = f.name()?.into_owned();
        if !seen.insert(name.to_ascii_lowercase()) {
            return Err("Membro ZIP duplicado (inclusive diferença de caixa)".into());
        }
        if f.unix_mode().is_some_and(|m| {
            let kind = m & 0o170000;
            kind != 0 && kind != 0o100000 && kind != 0o040000
        }) {
            return Err("Link ou membro especial no ZIP".into());
        }
        if name == "assets/" && f.is_dir() && f.size() == 0 {
            continue;
        }
        let allowed = matches!(
            name.as_str(),
            "manifest.json" | "backup-manifest.json" | "knowledge.sqlite3"
        ) || name.strip_prefix("assets/").is_some_and(safe_name);
        if !allowed || f.is_dir() || f.size() > FILE_LIMIT {
            return Err(format!("Caminho ou tamanho ZIP não permitido: {name}").into());
        }
        total = total.checked_add(f.size()).ok_or("Tamanho ZIP inválido")?;
        if total > TOTAL_LIMIT {
            return Err("ZIP descompactado excede 250 MiB".into());
        }
        let declared = f.size();
        let mut data = Vec::new();
        f.by_ref().take(FILE_LIMIT + 1).read_to_end(&mut data)?;
        if data.len() as u64 != declared {
            return Err("Tamanho ZIP divergente".into());
        }
        files.insert(name, data);
    }
    Ok(files)
}
pub fn write_zip(files: &BTreeMap<String, Vec<u8>>) -> StoreResult<Vec<u8>> {
    if files.len() > 1000
        || files.values().any(|b| b.len() as u64 > FILE_LIMIT)
        || files.values().map(|b| b.len() as u64).sum::<u64>() > TOTAL_LIMIT
    {
        return Err("Exportação excede limites ZIP (50/250 MiB, 1000 membros)".into());
    }
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        zip.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o600),
        )?;
        zip.write_all(bytes)?;
    }
    let bytes = zip.finish()?.into_inner();
    if bytes.len() as u64 > TOTAL_LIMIT {
        return Err("ZIP final excede 250 MiB".into());
    }
    Ok(bytes)
}
pub struct Package {
    pub value: Value,
    pub id: String,
    pub digest: String,
    pub raw_digest: String,
    pub kind: String,
    pub assets: Vec<(Asset, Vec<u8>)>,
}
impl Package {
    pub fn read(bytes: &[u8], format: &str, mode: &str) -> StoreResult<Self> {
        if format == "json" && bytes.len() > JSON_LIMIT {
            return Err("JSON excede 8 MiB".into());
        }
        let mut files = if format == "zip" {
            read_zip(bytes)?
        } else if format == "json" {
            BTreeMap::new()
        } else {
            return Err("Formato esperado: json ou zip".into());
        };
        let raw = if format == "zip" {
            files
                .remove("manifest.json")
                .ok_or("ZIP requer manifest.json na raiz")?
        } else {
            bytes.to_vec()
        };
        let value = strict_json(&raw)?;
        let kind = text(&value, "type")?.to_owned();
        if !["settings", "knowledge_update"].contains(&kind.as_str()) || kind != mode {
            return Err(
                "Operação e tipo do pacote não correspondem; restauração é separada".into(),
            );
        }
        if value["schema_version"] != 1 && value["schema_version"] != 2 {
            return Err("Schema deve ser v1 ou v2".into());
        }
        let allowed = if kind == "settings" {
            vec![
                "schema_version",
                "type",
                "profiles",
                "areas",
                "modules",
                "app",
                "appearance",
                "preferences",
            ]
        } else {
            vec![
                "schema_version",
                "type",
                "package_id",
                "profiles",
                "areas",
                "modules",
                "entries",
                "assets",
            ]
        };
        keys(&value, &allowed)?;
        if kind == "settings" && format != "json" {
            return Err("Configuração usa JSON sem anexos".into());
        }
        if value["schema_version"] == 1
            && (value.get("profiles").is_some()
                || list(&value, "entries")?
                    .iter()
                    .any(|e| e.get("blocks").is_some()))
        {
            return Err("Perfis e blocos exigem schema v2".into());
        }
        let mut assets = Vec::new();
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for a in list(&value, "assets")? {
            keys(a, &["id", "filename", "sha256", "media_type"])?;
            let id = text(a, "id")?;
            crate::blocks::validate_id(id)?;
            let filename = text(a, "filename")?;
            if !safe_name(filename)
                || !ids.insert(id)
                || !names.insert(filename.to_ascii_lowercase())
            {
                return Err("ID/nome de anexo inválido ou duplicado".into());
            }
            if format != "zip" {
                return Err("Anexos exigem ZIP; JSON não carrega bytes nem caminhos".into());
            }
            let data = files
                .remove(&format!("assets/{filename}"))
                .ok_or("Anexo do manifesto ausente no ZIP")?;
            let sha = text(a, "sha256")?;
            if hash(&data) != sha {
                return Err("SHA-256 de anexo divergente".into());
            }
            let mime = text(a, "media_type")?;
            validate_asset(filename, mime, &data)?;
            assets.push((
                Asset {
                    id: id.into(),
                    filename: filename.into(),
                    media_type: mime.into(),
                    sha256: sha.into(),
                    size: data.len() as u64,
                },
                data,
            ));
        }
        if !files.is_empty() {
            return Err("ZIP contém arquivos fora do manifesto".into());
        }
        let digest = hash(canonical(&value).as_bytes());
        let id = if kind == "settings" {
            format!("settings-{digest}")
        } else {
            text(&value, "package_id")?.to_string()
        };
        crate::blocks::validate_id(&id)?;
        Ok(Self {
            value,
            id,
            digest,
            raw_digest: hash(&raw),
            kind,
            assets,
        })
    }
}
