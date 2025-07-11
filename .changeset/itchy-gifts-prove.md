---
"zaku": patch
---

Standardize error handling

- Use structured error types
- Replace most `.unwrap()`/`.expect()` calls with `Result` based handling
- Use `CmdResult` for all tauri commands
