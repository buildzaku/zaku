---
"zaku": minor
---

Redesign store architecture

- Replace `RwLock` with `Mutex`
- Add `update` function with automatic persistence for all store types
- Replace global static caches with `OnceLock`
- Add tests for concurrent `SpaceBuf` operations
