<h1 align="center">
  <img src="./assets/readme/zaku_logo.svg" alt="Zaku" width="120px">
  <br>
  <span>Zaku</span>
</h1>
<p align="center">Fast, open-source API client with fangs.</p>
<p align="center">
  <img alt="Zaku" width="100%" src="./assets/readme/zaku_screenshot_dark.png">
</p>

## Installation

<h3>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./assets/readme/linux_dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="./assets/readme/linux_light.svg">
    <img src="./assets/readme/linux_light.svg" width="24px" align="top" alt="">
  </picture>
  <span>Linux</span>
</h3>

Install using the shell script:

```sh
curl -fsSL https://zaku.dev/install.sh | sh
```

Or download the tarball for [x86_64](https://api.zaku.dev/releases/stable/latest/linux-x86_64/download) or [aarch64](https://api.zaku.dev/releases/stable/latest/linux-aarch64/download).

<h3>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./assets/readme/apple_dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="./assets/readme/apple_light.svg">
    <img src="./assets/readme/apple_light.svg" width="24px" align="top" alt="">
  </picture>
  <span>macOS</span>
</h3>

Download the DMG for [Apple Silicon](https://api.zaku.dev/releases/stable/latest/macos-aarch64/download) or [Intel Chip](https://api.zaku.dev/releases/stable/latest/macos-x86_64/download). Requires macOS 14 or later.

After copying Zaku to Applications, open Terminal and run:

```sh
xattr -dr com.apple.quarantine /Applications/Zaku.app
```

This is required because Zaku isn't signed yet. Otherwise, macOS will refuse to open the app.

<h3>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="./assets/readme/microsoft_dark.svg">
    <source media="(prefers-color-scheme: light)" srcset="./assets/readme/microsoft_light.svg">
    <img src="./assets/readme/microsoft_light.svg" width="24px" align="top" alt="">
  </picture>
  <span>Windows</span>
</h3>

Download the installer for [Intel/AMD](https://api.zaku.dev/releases/stable/latest/windows-x86_64/download) or [ARM64](https://api.zaku.dev/releases/stable/latest/windows-aarch64/download). Requires Windows 11.

The installer is not signed yet. If Microsoft Defender SmartScreen appears, select **More info**, then **Run anyway**.

#### License

<sup>
Licensed under the <a href="LICENSE">GNU Affero General Public License,
Version 3.0 or later</a>.
</sup>
<br>
<sup>
Any contribution intentionally submitted for inclusion in this repository by
you shall be licensed under the GNU Affero General Public License, Version 3.0
or later, without any additional terms or conditions.
</sup>
