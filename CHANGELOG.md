# zaku

## 0.2.0

### Minor Changes

-   Setup app state and ability to create space on launch - [`08e83ca`](https://github.com/buildzaku/zaku/commit/08e83ca9748c2960cbf97dcf7b89736d2bcfaaa6) [#7](https://github.com/buildzaku/zaku/pull/7)

    -   Remove unused dependencies
    -   Pin all crates
    -   Implement get/set/delete active space invoke commands
    -   Get rid of problematic custom title bar

## 0.1.1

### Patch Changes

-   On release, sync snapcraft.yaml with latest metadata - [`bbc184d`](https://github.com/buildzaku/zaku/commit/bbc184d8550132139949e2318077b77f50574d35) [#5](https://github.com/buildzaku/zaku/pull/5)

-   add snapcraft.yaml for snap store distribution - [`59f700f`](https://github.com/buildzaku/zaku/commit/59f700f952cb4d6e9e38105aa4bf7b29c6ae003a)

-   custom titlebar and minor UI adjustments - [`065d356`](https://github.com/buildzaku/zaku/commit/065d3565e455f897689dbf664daf034d2487213e) [#6](https://github.com/buildzaku/zaku/pull/6)

-   avoid unnecessary binaries from being included in the release - [`a1decd6`](https://github.com/buildzaku/zaku/commit/a1decd62d16fac27893655ab95894f286f884a41)

## 0.1.0

### Minor Changes

-   feat(http): setup main layout and basic request functionality - [`2166dbe`](https://github.com/buildzaku/zaku/commit/2166dbeaa670aa99747bddf50ec1eaf243a46793)

    -   add ui primitives
    -   ability to pass query params and headers
    -   add code block to view raw and pretty response
    -   add preview tab for response
    -   resizable pane layout and dark/light theme
    -   update dependencies
    -   fix linting errors
    -   build app for all four major platforms & upload assets on release
    -   ability to force dispatch upload/reupload of assets
    -   format, lint and build app on pull requests
    -   add caching to speed up workflows
