use std::sync::Mutex;
use tauri::State;

use crate::types::ZakuState;

#[tauri::command]
pub fn get_zaku_state(state: State<Mutex<ZakuState>>) -> ZakuState {
    let zaku_state = state.lock().unwrap();

    return zaku_state.clone();
}
