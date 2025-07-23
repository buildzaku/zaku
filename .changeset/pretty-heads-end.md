---
"zaku": patch
---

Improve `CmdErr` structure and client error handling

- Replace `CmdErr` enum variants with `ErrorKind` including message & optional raw details
- Update client bindings and error handling to use new format
- Add centralized `emitCmdError` utility function
