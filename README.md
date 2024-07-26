# Zaku

## Trying it out

If you want to give Zaku a try, download it from the release assets based on your operating system.

### macOS Users

Zaku is not code signed yet, so you'll see this warning

![Screenshot 2024-07-26 at 20 52 06](https://github.com/user-attachments/assets/b8da8f66-6fa1-4cb2-bec4-71e75a98402a)

To work around this you need to remove the quarantine attribute flagged by Apple, read more about it [here](https://discussions.apple.com/thread/253714860).

Basically run this command from your terminal

```sh
xattr -c /Applications/Zaku.app
```
