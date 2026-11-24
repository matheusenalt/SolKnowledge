mod backup;
mod blocks;
mod models;
mod package;
mod search;
mod storage;
mod transfer;
mod transport;
mod visual;
mod workspace;

use models::*;
use std::sync::Mutex;
use storage::Store;
use tauri::Manager;

struct AppState(Mutex<Store>, Mutex<Option<transport::Pending>>);

#[derive(serde::Serialize)]
struct AppInfo {
    name: &'static str,
    version: &'static str,
    stage: &'static str,
}

#[tauri::command]
fn get_app_info() -> AppInfo {
    AppInfo {
        name: "SOL Knowledge",
        version: env!("CARGO_PKG_VERSION"),
        stage: "core",
    }
}

#[derive(serde::Serialize)]
struct IpcError {
    code: &'static str,
    message: String,
}

fn with_store<T>(
    state: &AppState,
    action: impl FnOnce(&mut Store) -> storage::StoreResult<T>,
) -> Result<T, IpcError> {
    let mut db = state.0.lock().map_err(|_| IpcError {
        code: "database_unavailable",
        message: "Banco indisponível. Reinicie o aplicativo sem apagar os dados.".into(),
    })?;
    action(&mut db).map_err(|error| {
        eprintln!("Falha em operação local: {error}");
        let message=if error.downcast_ref::<rusqlite::Error>().is_some() {
            "Não foi possível gravar ou consultar. Confira IDs únicos, referências existentes e atualize os dados antes de tentar novamente.".into()
        }else if error.downcast_ref::<serde_json::Error>().is_some(){"JSON inválido ou estrutura incompatível. Confira o formato e os campos.".into()}
        else {error.to_string()};
        IpcError {code:"operation_failed",message}
    })
}

#[tauri::command]
fn get_overview(state: tauri::State<'_, AppState>) -> Result<Overview, IpcError> {
    with_store(&state, |db| db.overview())
}
#[tauri::command]
fn search_knowledge(
    state: tauri::State<'_, AppState>,
    request: SearchRequest,
) -> Result<Vec<SearchResult>, IpcError> {
    with_store(&state, |db| db.search(request))
}
#[tauri::command]
fn get_entry(state: tauri::State<'_, AppState>, id: String) -> Result<EntryDetail, IpcError> {
    with_store(&state, |db| db.visit_entry(&id))
}
#[tauri::command]
fn get_entries_index(state: tauri::State<'_, AppState>) -> Result<Vec<EntryIndex>, IpcError> {
    with_store(&state, |db| db.entries_index())
}
#[tauri::command]
fn list_vocabulary(state: tauri::State<'_, AppState>) -> Result<Vec<VocabularyTerm>, IpcError> {
    with_store(&state, |db| db.vocabulary())
}
#[tauri::command]
fn save_vocabulary_term(
    state: tauri::State<'_, AppState>,
    input: TermInput,
) -> Result<i64, IpcError> {
    with_store(&state, |db| db.save_term(input))
}
#[tauri::command]
fn delete_vocabulary_term(
    state: tauri::State<'_, AppState>,
    kind: TermKind,
    id: i64,
) -> Result<(), IpcError> {
    with_store(&state, |db| db.delete_term(kind, id))
}

#[tauri::command]
fn get_catalog(state: tauri::State<'_, AppState>) -> Result<Catalog, IpcError> {
    with_store(&state, |db| db.catalog())
}
#[tauri::command]
fn resolve_presentation(
    state: tauri::State<'_, AppState>,
    module_id: String,
) -> Result<Presentation, IpcError> {
    with_store(&state, |db| db.resolve_presentation(&module_id))
}
#[tauri::command]
fn save_presentation(
    state: tauri::State<'_, AppState>,
    input: PresentationInput,
) -> Result<(), IpcError> {
    with_store(&state, |db| db.save_presentation(input))
}
#[tauri::command]
fn create_profile(state: tauri::State<'_, AppState>, json: String) -> Result<(), IpcError> {
    with_store(&state, |db| db.create_profile(&json))
}
#[tauri::command]
fn create_area(state: tauri::State<'_, AppState>, input: NewArea) -> Result<(), IpcError> {
    with_store(&state, |db| db.create_area(input))
}
#[tauri::command]
fn create_module(state: tauri::State<'_, AppState>, input: NewModule) -> Result<(), IpcError> {
    with_store(&state, |db| db.create_module(input))
}
#[tauri::command]
fn browse_entries(
    state: tauri::State<'_, AppState>,
    request: BrowseRequest,
) -> Result<CatalogPage, IpcError> {
    with_store(&state, |db| db.browse(request))
}
#[tauri::command]
fn set_favorite(
    state: tauri::State<'_, AppState>,
    id: String,
    value: bool,
) -> Result<(), IpcError> {
    with_store(&state, |db| db.favorite(&id, value))
}
#[tauri::command]
fn get_history(state: tauri::State<'_, AppState>) -> Result<History, IpcError> {
    with_store(&state, |db| db.history())
}
#[tauri::command]
fn save_entry(
    state: tauri::State<'_, AppState>,
    input: EntryInput,
) -> Result<EntryDetail, IpcError> {
    with_store(&state, |db| db.save_entry(input))
}
#[tauri::command]
fn save_block(
    state: tauri::State<'_, AppState>,
    input: BlockInput,
) -> Result<EntryDetail, IpcError> {
    with_store(&state, |db| db.save_block(input))
}
#[tauri::command]
fn reorder_blocks(
    state: tauri::State<'_, AppState>,
    input: BlockOrder,
) -> Result<EntryDetail, IpcError> {
    with_store(&state, |db| db.reorder_blocks(input))
}

#[tauri::command]
fn get_asset(state: tauri::State<'_, AppState>, id: String) -> Result<visual::Asset, IpcError> {
    with_store(&state, |db| db.asset(&id))
}
#[tauri::command]
fn read_asset(state: tauri::State<'_, AppState>, id: String) -> Result<Vec<u8>, IpcError> {
    with_store(&state, |db| db.read_asset(&id))
}
#[tauri::command]
fn put_asset(
    state: tauri::State<'_, AppState>,
    input: visual::AssetInput,
) -> Result<visual::Asset, IpcError> {
    with_store(&state, |db| db.put_asset(input))
}
#[tauri::command]
fn list_backups(state: tauri::State<'_, AppState>) -> Result<Vec<backup::StoredBackup>, IpcError> {
    with_store(&state, |db| db.list_backups())
}
#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, IpcError> {
    with_store(&state, |db| db.export_settings())
}
#[tauri::command]
fn save_export(
    state: tauri::State<'_, AppState>,
    kind: String,
    id: Option<String>,
) -> Result<bool, IpcError> {
    let (name, extension) = match kind.as_str() {
        "settings" => ("sol-settings.json".to_string(), "json"),
        "entry" => ("sol-entry.json".to_string(), "json"),
        "entry_zip" => ("sol-entry.zip".to_string(), "zip"),
        "backup" | "stored_backup" => ("sol-backup.zip".to_string(), "zip"),
        _ => {
            return Err(IpcError {
                code: "invalid_export",
                message: "Exportação desconhecida".into(),
            })
        }
    };
    let Some(path) = rfd::FileDialog::new()
        .set_title("Salvar exportação SOL Knowledge")
        .set_file_name(name)
        .add_filter("SOL Knowledge", &[extension])
        .save_file()
    else {
        return Ok(false);
    };
    with_store(&state, |db| {
        let parent = path.parent().ok_or("Destino inválido")?;
        if parent
            .canonicalize()?
            .starts_with(db.data_root()?.canonicalize()?)
        {
            return Err("Exporte fora do diretório interno de dados do aplicativo".into());
        }
        let bytes = match kind.as_str() {
            "settings" => serde_json::to_vec_pretty(&db.export_settings()?)?,
            "entry" => serde_json::to_vec_pretty(
                &db.export_record(id.as_deref().ok_or("ID obrigatório")?)?,
            )?,
            "entry_zip" => db.export_record_archive(id.as_deref().ok_or("ID obrigatório")?)?,
            "backup" => db.backup_bytes()?,
            "stored_backup" => db.stored_backup(id.as_deref().ok_or("ID obrigatório")?)?,
            _ => return Err("Exportação desconhecida".into()),
        };
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        use std::io::Write;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist(&path)?;
        Ok(true)
    })
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let path = app.path().app_data_dir()?.join("knowledge.sqlite3");
            let db = Store::open(&path).map_err(|error| {
                eprintln!("Não foi possível abrir a base local: {error}");
                std::io::Error::other(
                    "Falha ao abrir o banco local; os dados existentes foram preservados",
                )
            })?;
            app.manage(AppState(Mutex::new(db), Mutex::new(None)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_asset,
            read_asset,
            put_asset,
            transport::choose_transfer,
            transport::cancel_transfer,
            transport::confirm_import,
            transport::confirm_restore,
            list_backups,
            get_settings,
            save_export,
            get_app_info,
            get_overview,
            search_knowledge,
            get_entry,
            get_entries_index,
            list_vocabulary,
            save_vocabulary_term,
            delete_vocabulary_term,
            get_catalog,
            resolve_presentation,
            save_presentation,
            create_profile,
            create_area,
            create_module,
            browse_entries,
            set_favorite,
            get_history,
            save_entry,
            save_block,
            reorder_blocks
        ])
        .run(tauri::generate_context!())
        .expect("Não foi possível iniciar o SOL Knowledge");
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_restart;
#[cfg(test)]
mod tests_workspace;

#[cfg(test)]
mod tests_transfer;
