---
"zaku": minor
---

Improve store architecture with scoped stores

- Fix tests polluting app data directory
- Move from `SpaceBuf`/`SpaceCookies`/`SpaceSettings` to typed store pattern
- Replace managed `tauri::AppHandle` state with custom state implementation
- Consolidate store utilities and improve path management
