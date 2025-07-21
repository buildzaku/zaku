---
"zaku": patch
---

Refactor create collections and requests API

- Add test suite for request and utils module
- Only allow alphabetics, ascii digits and '-' for filesystem name
- Throw sanitization error if filesystem name is a reserved name or empty
