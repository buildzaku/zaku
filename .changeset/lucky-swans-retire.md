---
"zaku": minor
---

Restructure store architecture with scoped stores

- Add `UserSettingsStore` for global user preferences
- Move from `SpaceBuf`/`SpaceCookies`/`SpaceSettings` to typed store pattern
- Consolidate store utilities and improve path management
