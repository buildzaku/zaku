---
"zaku": patch
---

Improve client error handling with `CmdErr` type

- Replace generic `CmdErr::Err` with `ErrorKind` enum
- Add human-readable `message` and optional raw `details`
