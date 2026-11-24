//! Native file selection keeps large ZIP buffers out of JSON IPC and never accepts a caller path.
use crate::{
    backup::{RestoreApproval, StoredBackup},
    package,
    storage::StoreResult,
    transfer::{Applied, ImportApproval, ImportInput},
    with_store, AppState, IpcError,
};
use serde::Serialize;
use serde_json::Value;
use std::io::Read;
pub enum PendingData {
    Import(ImportInput),
    Restore(Vec<u8>),
}
pub struct Pending {
    pub ticket: String,
    pub data: PendingData,
}
#[derive(Serialize)]
pub struct Selection {
    ticket: String,
    mode: String,
    preview: Value,
}
fn pending_error() -> IpcError {
    IpcError {
        code: "preview_expired",
        message: "Prévia encerrada. Selecione o arquivo novamente.".into(),
    }
}
#[tauri::command]
pub fn choose_transfer(
    state: tauri::State<'_, AppState>,
    mode: String,
) -> Result<Option<Selection>, IpcError> {
    let extensions = match mode.as_str() {
        "settings" => vec!["json"],
        "knowledge_update" => vec!["json", "zip"],
        "restore" => vec!["zip"],
        _ => return Err(pending_error()),
    };
    let Some(path) = rfd::FileDialog::new()
        .set_title("Selecionar pacote SOL Knowledge")
        .add_filter("SOL Knowledge", &extensions)
        .pick_file()
    else {
        return Ok(None);
    };
    let selection = with_store(&state, |db| -> StoreResult<(Value, PendingData)> {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .ok_or("Extensão obrigatória")?
            .to_ascii_lowercase();
        if !extensions.contains(&ext.as_str()) {
            return Err("Extensão incompatível com esta operação".into());
        }
        let meta = std::fs::symlink_metadata(&path)?;
        let limit = if ext == "json" {
            package::JSON_LIMIT as u64
        } else {
            package::TOTAL_LIMIT
        };
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > limit {
            return Err("Pacote inválido ou acima do limite".into());
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&path)?
            .take(limit + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            return Err("Pacote excede limite".into());
        }
        if mode == "restore" {
            let preview = serde_json::to_value(db.preview_restore(&bytes)?)?;
            Ok((preview, PendingData::Restore(bytes)))
        } else {
            let input = ImportInput {
                format: ext,
                mode: mode.clone(),
                bytes,
            };
            let preview = serde_json::to_value(db.preview_package(&input)?)?;
            Ok((preview, PendingData::Import(input)))
        }
    })?;
    let ticket = crate::backup::unique_id();
    let mut pending = state.1.lock().map_err(|_| pending_error())?;
    *pending = Some(Pending {
        ticket: ticket.clone(),
        data: selection.1,
    });
    Ok(Some(Selection {
        ticket,
        mode,
        preview: selection.0,
    }))
}
#[tauri::command]
pub fn cancel_transfer(state: tauri::State<'_, AppState>, ticket: String) -> Result<(), IpcError> {
    let mut pending = state.1.lock().map_err(|_| pending_error())?;
    if pending.as_ref().is_some_and(|p| p.ticket == ticket) {
        pending.take();
    }
    Ok(())
}
#[tauri::command]
pub fn confirm_import(
    state: tauri::State<'_, AppState>,
    ticket: String,
    approval: ImportApproval,
) -> Result<Applied, IpcError> {
    let mut pending = state.1.lock().map_err(|_| pending_error())?;
    let selected = pending
        .as_ref()
        .filter(|p| p.ticket == ticket)
        .ok_or_else(pending_error)?;
    let PendingData::Import(input) = &selected.data else {
        return Err(pending_error());
    };
    let result = with_store(&state, |db| db.apply_package(input, approval))?;
    pending.take();
    Ok(result)
}
#[tauri::command]
pub fn confirm_restore(
    state: tauri::State<'_, AppState>,
    ticket: String,
    approval: RestoreApproval,
) -> Result<StoredBackup, IpcError> {
    let mut pending = state.1.lock().map_err(|_| pending_error())?;
    let selected = pending
        .as_ref()
        .filter(|p| p.ticket == ticket)
        .ok_or_else(pending_error)?;
    let PendingData::Restore(bytes) = &selected.data else {
        return Err(pending_error());
    };
    let result = with_store(&state, |db| db.restore_backup(bytes, approval))?;
    pending.take();
    Ok(result)
}
