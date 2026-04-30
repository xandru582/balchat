//! Desktop entry point. La lógica vive en `lib.rs` para que mobile pueda
//! consumirla desde `JNI_OnLoad` vía `#[tauri::mobile_entry_point]`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    balchat_mobile::run()
}
