---
"zaku": patch
---

Improve CI release workflow

- Standardize build artifact naming with platform identifiers
- Use matrix strategy for all platform builds
- Defer artifact uploads until all builds succeed
- Graceful git tag creation
