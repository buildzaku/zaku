# zaku

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
