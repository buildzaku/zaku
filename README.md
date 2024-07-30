<br>

<p align="center">
    <img width="82px" src="./assets/zaku-icon.png" alt="Zaku Icon">
</p>

<p align="center">
  <a href="https://zaku.app" target="_blank">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="./assets/zaku-dark.svg">
      <source media="(prefers-color-scheme: light)" srcset="./assets/zaku-light.svg">
      <img alt="Zaku" src="./assets/zaku-light.svg" height="19px" style="max-width: 100%;">
    </picture>
  </a>
</p>

<h5 align="center">Fast, open-source API client with fangs</h5>

<p align="center">
    <a href="https://github.com/buildzaku/zaku/actions/workflows/release.yml" target="_blank"><img alt="Build Status" src="https://img.shields.io/github/actions/workflow/status/buildzaku/zaku/release.yml?style=flat&logo=github&labelColor=%2324292e" /></a>
    <a href="https://github.com/buildzaku/zaku/releases/latest" target="_blank"><img alt="Release" src="https://img.shields.io/github/v/release/buildzaku/zaku?sort=semver&style=flat&labelColor=%2324292e"></a>
    <a href="https://github.com/buildzaku/zaku/blob/main/LICENSE.md" target="_blank"><img alt="License" src="https://img.shields.io/github/license/buildzaku/zaku?style=flat&labelColor=%2324292e&color=%2354d024"></a>
</p>

> [!WARNING]
> Zaku is in early stages of development, expect breaking changes.
> Also, we're not ready for contributions just yet.
> Thanks for checking it out and stay tuned for updates!

## Test it out

If you want to give Zaku a try, download it from the [release assets](https://github.com/buildzaku/zaku/releases/latest) based on your operating system.

### macOS Users

Zaku is not code signed yet, so you'll see this warning

![Screenshot 2024-07-26 at 20 52 06](https://github.com/user-attachments/assets/b8da8f66-6fa1-4cb2-bec4-71e75a98402a)

To work around this you need to remove the quarantine attribute flagged by Apple, read more about it [here](https://discussions.apple.com/thread/253714860).

<b>TL;DR:</b> After installing Zaku, run this command from your terminal

```sh
xattr -c /Applications/Zaku.app
```

## License

Zaku is licensed under the [MIT license](https://github.com/buildzaku/zaku/blob/main/LICENSE.md).
