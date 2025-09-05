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

<p align="center">
  <img alt="Zaku application" width="100%" src="./assets/interface.png">
</p>

> [!WARNING]
> Zaku is in early stages of development, expect breaking changes.

<h2>Installation</h2>

<h4>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./assets/apple-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="./assets/apple-light.svg">
    <img src="./assets/apple-light.svg" width="20px" align="top">
  </picture>
  <span>macOS</span>
</h4>

Download for [Arm (Apple Silicon)](https://github.com/buildzaku/zaku/releases/latest/download/zaku-aarch64-apple-darwin.dmg) or [x86 (Intel)](https://github.com/buildzaku/zaku/releases/latest/download/zaku-x86_64-apple-darwin.dmg)

After installing, run this command from your terminal:

```sh
xattr -c /Applications/Zaku.app
```

This is required because Zaku is not code signed yet. [Read more](https://discussions.apple.com/thread/253714860)

<h4>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./assets/linux-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="./assets/linux-light.svg">
    <img src="./assets/linux-light.svg" width="20px" align="top">
  </picture>
  <span>Linux</span>
</h4>

Download the [.deb package](https://github.com/buildzaku/zaku/releases/latest/download/zaku-x86_64-unknown-linux-gnu.deb) for x86 Ubuntu/Debian

From your terminal, navigate to the download location and run:

```sh
sudo apt install ./zaku-x86_64-unknown-linux-gnu.deb
```

Or install via Snap:

```sh
sudo snap install zaku
```

<h4>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./assets/microsoft-dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="./assets/microsoft-light.svg">
    <img src="./assets/microsoft-light.svg" width="20px" align="top">
  </picture>
  <span>Windows</span>
</h4>

Download the [.exe file](https://github.com/buildzaku/zaku/releases/latest/download/zaku-x86_64-pc-windows-msvc.exe) or [MSI package](https://github.com/buildzaku/zaku/releases/latest/download/zaku-x86_64-pc-windows-msvc.msi)

Launch the installer and follow the prompts.

## Contributing

Checkout the [contributing guide](./.github/CONTRIBUTING.md).

## License

Zaku is licensed under the [MIT license](./LICENSE.md).
