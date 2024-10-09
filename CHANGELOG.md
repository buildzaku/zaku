# zaku

## 0.4.0

### Minor Changes

-   Tree view for collections and requests - [`4229470`](https://github.com/buildzaku/zaku/commit/42294706ac7bfd74d361ebb58b75a525f9f1f70d) [#18](https://github.com/buildzaku/zaku/pull/18)

-   Ability to create new requests and collections - [`d1317c7`](https://github.com/buildzaku/zaku/commit/d1317c7f9b4215cb2986791e4c9af98218c5203a) [#22](https://github.com/buildzaku/zaku/pull/22)

    -   With support for nested input

### Patch Changes

-   Handlers for request and response panes - [`874de99`](https://github.com/buildzaku/zaku/commit/874de99f5f31b60f1bd01f0cef49fc6523cdb8af) [#20](https://github.com/buildzaku/zaku/pull/20)

    -   Improve colors for dark theme
    -   Fix request method colors

## 0.3.1

### Patch Changes

-   Separate `error` and `message` in `ZakuError` struct - [`9b6315f`](https://github.com/buildzaku/zaku/commit/9b6315f30ccbee053df4125a12dc3cdfa33c1118)

-   Display toast if space is invalid on switch - [`0b6a48f`](https://github.com/buildzaku/zaku/commit/0b6a48fe7abd9bf4ef0cc77a7b06eaa3f56ad178) [#17](https://github.com/buildzaku/zaku/pull/17)

## 0.3.0

### Minor Changes

-   Ability to handle multiple spaces - [`b82b68e`](https://github.com/buildzaku/zaku/commit/b82b68e82a70bbb747eab66513d074a7875cec1e) [#14](https://github.com/buildzaku/zaku/pull/14)

## 0.2.3

### Patch Changes

-   Implement dispatch, check & request commands for notification - [`8c12516`](https://github.com/buildzaku/zaku/commit/8c12516f3f773a9336c7161f947b93980293066b) [#12](https://github.com/buildzaku/zaku/pull/12)

## 0.2.2

### Patch Changes

-   Add option to open existing space from filesystem - [`834139d`](https://github.com/buildzaku/zaku/commit/834139dd5c9747e8e49dfd735f9d67250831ccdb) [#11](https://github.com/buildzaku/zaku/pull/11)

-   Add platform specific global shortcuts to toggle devtools - [`3715feb`](https://github.com/buildzaku/zaku/commit/3715feba25d9aaf737e951f3a993e4b3280fb3ba)

## 0.2.1

### Patch Changes

-   Fix URL capability to allow requests on all ports - [`d97ef73`](https://github.com/buildzaku/zaku/commit/d97ef73148e6fc7efd943cca5d3e5e27ed8ed8c6)

-   Fix incorrect reactive check for active space - [`fc8cc61`](https://github.com/buildzaku/zaku/commit/fc8cc615248fec31781e6f7cc538905f676faa68)

## 0.2.0

### Minor Changes

-   Setup app state and ability to create space on launch - [`08e83ca`](https://github.com/buildzaku/zaku/commit/08e83ca9748c2960cbf97dcf7b89736d2bcfaaa6) [#7](https://github.com/buildzaku/zaku/pull/7)

    -   Remove unused dependencies
    -   Pin all crates
    -   Implement get/set/delete active space invoke commands
    -   Get rid of problematic custom title bar

## 0.1.1

### Patch Changes

-   On release, sync `snapcraft.yaml` with latest metadata - [`bbc184d`](https://github.com/buildzaku/zaku/commit/bbc184d8550132139949e2318077b77f50574d35) [#5](https://github.com/buildzaku/zaku/pull/5)

-   Add `snapcraft.yaml` for snap store distribution - [`59f700f`](https://github.com/buildzaku/zaku/commit/59f700f952cb4d6e9e38105aa4bf7b29c6ae003a)

-   Custom titlebar and minor UI adjustments - [`065d356`](https://github.com/buildzaku/zaku/commit/065d3565e455f897689dbf664daf034d2487213e) [#6](https://github.com/buildzaku/zaku/pull/6)

-   Avoid unnecessary binaries from being included in the release - [`a1decd6`](https://github.com/buildzaku/zaku/commit/a1decd62d16fac27893655ab95894f286f884a41)

## 0.1.0

### Minor Changes

-   Setup main layout and basic request functionality - [`2166dbe`](https://github.com/buildzaku/zaku/commit/2166dbeaa670aa99747bddf50ec1eaf243a46793)

    -   Add UI primitives
    -   Ability to pass query params and headers
    -   Add code block to view raw and pretty response
    -   Add preview tab for response
    -   Resizable pane layout and dark/light theme
    -   Update dependencies
    -   Fix linting errors
    -   Build app for all four major platforms & upload assets on release
    -   Ability to force dispatch upload/reupload of assets
    -   Format, lint and build app on pull requests
    -   Add caching to speed up workflows
