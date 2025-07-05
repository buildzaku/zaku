use std::sync::Mutex;
use tauri::State;

use crate::models::zaku::ZakuState;

#[specta::specta]
#[tauri::command]
pub fn get_zaku_state(state: State<Mutex<ZakuState>>) -> Result<ZakuState, String> {
    match state.lock() {
        Ok(zaku_state) => Ok(zaku_state.clone()),
        Err(e) => Err(format!("State lock error: {}", e)),
    }
}
