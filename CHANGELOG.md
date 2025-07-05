# zaku

## 0.5.1

### Patch Changes

- Replace ts-rs with specta for generating typescript bindings - [`8b11c0b`](https://github.com/buildzaku/zaku/commit/8b11c0b781c1a906caaf656deed756e795bee2f8) [#35](https://github.com/buildzaku/zaku/pull/35)

## 0.5.0

### Minor Changes

- Add support for basic request body - [`d34b5c4`](https://github.com/buildzaku/zaku/commit/d34b5c4827cc7f83e4193e5310d93f24fe8203c5) [#29](https://github.com/buildzaku/zaku/pull/29)

    - json, xml, html and plaintext

### Patch Changes

- Fix input focus loss when editing param/header key - [`77df425`](https://github.com/buildzaku/zaku/commit/77df425243fa05a323252f3450b9607bf746bbe5) [#32](https://github.com/buildzaku/zaku/pull/32)

- Automatically set `Content-Type` header based on the selected body type, unless it has been manually set - [`2239234`](https://github.com/buildzaku/zaku/commit/22392348443d74a5ea0450c671a6407d72d2bb80) [#31](https://github.com/buildzaku/zaku/pull/31)

- Fix broken editor state on switching requests - [`d56bfab`](https://github.com/buildzaku/zaku/commit/d56bfabfd1fbfb861547db7540294cae67b4afb9) [#33](https://github.com/buildzaku/zaku/pull/33)

    - make request config props optional to prevent unnecessary serialization
    - reset active request on switching space

- Use refs to avoid broken updates to active request caused by debounced state changes - [`9478142`](https://github.com/buildzaku/zaku/commit/94781424793095d2aae5f776b3435a340eb2cac1) [#34](https://github.com/buildzaku/zaku/pull/34)

## 0.4.0

### Minor Changes

- Tree view for collections and requests - [`4229470`](https://github.com/buildzaku/zaku/commit/42294706ac7bfd74d361ebb58b75a525f9f1f70d) [#18](https://github.com/buildzaku/zaku/pull/18)

- Refactor to svelte 5 runes - [`fe87a84`](https://github.com/buildzaku/zaku/commit/fe87a84a38aa0ef9b4ca7604b117140a0d93411f) [#27](https://github.com/buildzaku/zaku/pull/27)

- Ability to create new requests and collections - [`d1317c7`](https://github.com/buildzaku/zaku/commit/d1317c7f9b4215cb2986791e4c9af98218c5203a) [#22](https://github.com/buildzaku/zaku/pull/22)

    - With support for nested input

- Custom store for persistence - [`6c8423b`](https://github.com/buildzaku/zaku/commit/6c8423bbe11bd91eec076370d7c2ac1758dfe309) [#24](https://github.com/buildzaku/zaku/pull/24)

- Persist changes to space buffer and filesystem - [`c597541`](https://github.com/buildzaku/zaku/commit/c59754178f22e9db28d472d2b40fc716982362c7) [#28](https://github.com/buildzaku/zaku/pull/28)

    - Use space buffer to preserve changes in case app is closed
    - Write changes to filesystem on `Cmd+s`/`Ctrl+s`

### Patch Changes

- Fix lint warnings and incorrect formatting after version bump - [`4b3cdf5`](https://github.com/buildzaku/zaku/commit/4b3cdf54b94e56871a7cb7df4fd7d497f042c372)

- Preserve moved tree item on drag and drop - [`af5f535`](https://github.com/buildzaku/zaku/commit/af5f5350ab7203976dfc17282c271b1f03b940a2) [#25](https://github.com/buildzaku/zaku/pull/25)

- Fix blank window on Linux with Nvidia GPU - [`c0203dc`](https://github.com/buildzaku/zaku/commit/c0203dc13c7a703e448af78a8f5060676212588a) [#23](https://github.com/buildzaku/zaku/pull/23)

- Handlers for request and response panes - [`874de99`](https://github.com/buildzaku/zaku/commit/874de99f5f31b60f1bd01f0cef49fc6523cdb8af) [#20](https://github.com/buildzaku/zaku/pull/20)

    - Improve colors for dark theme
    - Fix request method colors

- Add generated `bindings.ts` file to `.prettierignore` to prevent formatting discrepancies - [`067b4f9`](https://github.com/buildzaku/zaku/commit/067b4f9769dbf21024e7cc5fec7997ca79b35513)

- Remove strict pinning of cargo dependencies to avoid version mismatches - [`e3e980c`](https://github.com/buildzaku/zaku/commit/e3e980c588b0ed589be8bb947354e07dc0f43dd0)

- Traffic lights inset for macOS - [`b8480ff`](https://github.com/buildzaku/zaku/commit/b8480ff52a7c9b5064e28f70e5e64a4e1b5d0133) [#26](https://github.com/buildzaku/zaku/pull/26)

## 0.3.1

### Patch Changes

- Separate `error` and `message` in `ZakuError` struct - [`9b6315f`](https://github.com/buildzaku/zaku/commit/9b6315f30ccbee053df4125a12dc3cdfa33c1118)

- Display toast if space is invalid on switch - [`0b6a48f`](https://github.com/buildzaku/zaku/commit/0b6a48fe7abd9bf4ef0cc77a7b06eaa3f56ad178) [#17](https://github.com/buildzaku/zaku/pull/17)

## 0.3.0

### Minor Changes

- Ability to handle multiple spaces - [`b82b68e`](https://github.com/buildzaku/zaku/commit/b82b68e82a70bbb747eab66513d074a7875cec1e) [#14](https://github.com/buildzaku/zaku/pull/14)

## 0.2.3

### Patch Changes

- Implement dispatch, check & request commands for notification - [`8c12516`](https://github.com/buildzaku/zaku/commit/8c12516f3f773a9336c7161f947b93980293066b) [#12](https://github.com/buildzaku/zaku/pull/12)

## 0.2.2

### Patch Changes

- Add option to open existing space from filesystem - [`834139d`](https://github.com/buildzaku/zaku/commit/834139dd5c9747e8e49dfd735f9d67250831ccdb) [#11](https://github.com/buildzaku/zaku/pull/11)

- Add platform specific global shortcuts to toggle devtools - [`3715feb`](https://github.com/buildzaku/zaku/commit/3715feba25d9aaf737e951f3a993e4b3280fb3ba)

## 0.2.1

### Patch Changes

- Fix URL capability to allow requests on all ports - [`d97ef73`](https://github.com/buildzaku/zaku/commit/d97ef73148e6fc7efd943cca5d3e5e27ed8ed8c6)

- Fix incorrect reactive check for active space - [`fc8cc61`](https://github.com/buildzaku/zaku/commit/fc8cc615248fec31781e6f7cc538905f676faa68)

## 0.2.0

### Minor Changes

- Setup app state and ability to create space on launch - [`08e83ca`](https://github.com/buildzaku/zaku/commit/08e83ca9748c2960cbf97dcf7b89736d2bcfaaa6) [#7](https://github.com/buildzaku/zaku/pull/7)

    - Remove unused dependencies
    - Pin all crates
    - Implement get/set/delete active space invoke commands
    - Get rid of problematic custom title bar

## 0.1.1

### Patch Changes

- On release, sync `snapcraft.yaml` with latest metadata - [`bbc184d`](https://github.com/buildzaku/zaku/commit/bbc184d8550132139949e2318077b77f50574d35) [#5](https://github.com/buildzaku/zaku/pull/5)

- Add `snapcraft.yaml` for snap store distribution - [`59f700f`](https://github.com/buildzaku/zaku/commit/59f700f952cb4d6e9e38105aa4bf7b29c6ae003a)

- Custom titlebar and minor UI adjustments - [`065d356`](https://github.com/buildzaku/zaku/commit/065d3565e455f897689dbf664daf034d2487213e) [#6](https://github.com/buildzaku/zaku/pull/6)

- Avoid unnecessary binaries from being included in the release - [`a1decd6`](https://github.com/buildzaku/zaku/commit/a1decd62d16fac27893655ab95894f286f884a41)

## 0.1.0

### Minor Changes

- Setup main layout and basic request functionality - [`2166dbe`](https://github.com/buildzaku/zaku/commit/2166dbeaa670aa99747bddf50ec1eaf243a46793)

    - Add UI primitives
    - Ability to pass query params and headers
    - Add code block to view raw and pretty response
    - Add preview tab for response
    - Resizable pane layout and dark/light theme
    - Update dependencies
    - Fix linting errors
    - Build app for all four major platforms & upload assets on release
    - Ability to force dispatch upload/reupload of assets
    - Format, lint and build app on pull requests
    - Add caching to speed up workflows
