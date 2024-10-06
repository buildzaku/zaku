use std::sync::Mutex;
use tauri::State;

use crate::models::zaku::ZakuState;

#[tauri::command(rename_all = "snake_case")]
pub fn get_zaku_state(state: State<Mutex<ZakuState>>) -> ZakuState {
    let zaku_state = state.lock().unwrap();

    return zaku_state.clone();
}
