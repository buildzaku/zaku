# zaku

## 0.8.0

### Minor Changes

- Implement http handler command using `reqwest` - [`7f047ed`](https://github.com/buildzaku/zaku/commit/7f047ed24f7c94b5f7770806bfac0fe5036c69f7)
    - Captures response status, headers, cookies, size & elapsed time

- Refactor create collections and requests API - [`5d5995d`](https://github.com/buildzaku/zaku/commit/5d5995df26bd8b6b00ab1e778c3968d4e1e1fcb8)
    - Add test suite for request and utils module
    - Only allow alphabetics, ascii digits and '-' for filesystem name
    - Throw sanitization error if filesystem name is a reserved name or empty

- feat(http): setup main layout and basic request functionality - [`f78de10`](https://github.com/buildzaku/zaku/commit/f78de10e6c1bb0da92be878b007bc6ce4e675da9)
    - add ui primitives
    - ability to pass query params and headers
    - add code block to view raw and pretty response
    - add preview tab for response
    - resizable pane layout and dark/light theme
    - update dependencies
    - fix linting errors
    - build app for all four major platforms & upload assets on release
    - ability to force dispatch upload/reupload of assets
    - format, lint and build app on pull requests
    - add caching to speed up workflows

- Add support for cookies with persistence (at space-level) - [`aefaf21`](https://github.com/buildzaku/zaku/commit/aefaf2142d1b3d92676dddb0ee35a5cf560aa70d)

### Patch Changes

- Prevent double hyphens in sanitized path segments - [`dd2eb48`](https://github.com/buildzaku/zaku/commit/dd2eb48e8a837f40efd095533bf72b5dc468dcf2)
    - Update tests' path segments with self-describing names

- Separate `error` and `message` in `ZakuError` struct - [`1c4edef`](https://github.com/buildzaku/zaku/commit/1c4edef09662c5cc14c1d8e1aef54d91bd1040c8)

- Fix URL capability to allow requests on all ports - [`3d6abd4`](https://github.com/buildzaku/zaku/commit/3d6abd4e91135cd971729cc9335567a7ed59adef)

- On release, sync snapcraft.yaml with latest metadata - [`c520ef7`](https://github.com/buildzaku/zaku/commit/c520ef725dca8ba0cf65f3cad8b8085e90d37c6e)

- Replace ts-rs with specta for generating typescript bindings - [`599de01`](https://github.com/buildzaku/zaku/commit/599de01047390ab236af39f41c48e54bc03245b2)

- Use fine-grained reactivity for buffer writes - [`fe82e3d`](https://github.com/buildzaku/zaku/commit/fe82e3d54b6fc98d572c934d3d86a68a8eb36ee8)
    - Resolve HTTP status/response triggering space buffer writes

- add snapcraft.yaml for snap store distribution - [`0c08e0f`](https://github.com/buildzaku/zaku/commit/0c08e0ff88c6a7312fa4a8f0eabae046aafca78f)

- Standardize error handling - [`a51fef6`](https://github.com/buildzaku/zaku/commit/a51fef61deb78f84673d4b26951b7eded763b468)
    - Use structured error types
    - Replace most `.unwrap()`/`.expect()` calls with `Result` based handling
    - Use `CmdResult` for all tauri commands

- Improve CI release workflow - [`aab4b78`](https://github.com/buildzaku/zaku/commit/aab4b78e4178fe78ed31bfcd33284dc48c6e3886)
    - Standardize build artifact naming with platform identifiers
    - Use matrix strategy for all platform builds
    - Defer artifact uploads until all builds succeed
    - Graceful git tag creation

- Fix incorrect reactive check for active space - [`9e1915d`](https://github.com/buildzaku/zaku/commit/9e1915dd947880e42eebd50966497f6cdfda185c)

- custom titlebar and minor UI adjustments - [`b0bb8e0`](https://github.com/buildzaku/zaku/commit/b0bb8e0359c358af62f3428cc01ebe79f5bcd63f)

- Remove duplicate query parameter values added to the request - [`bcd0432`](https://github.com/buildzaku/zaku/commit/bcd0432ae13bcb319209cf3fa767859aeda122de)

- Add unit tests for collection module - [`72d9b66`](https://github.com/buildzaku/zaku/commit/72d9b669f8e4144532ee11ee615a8f885073122a)

- Improve `CmdErr` structure and client error handling - [`9e684f9`](https://github.com/buildzaku/zaku/commit/9e684f94e0153bdc3995f6523d82b4bd090c9d05)
    - Replace `CmdErr` enum variants with `ErrorKind` including message & optional raw details
    - Update client bindings and error handling to use new format
    - Add centralized `emitCmdError` utility function

- Improve keyboard focus visibility - [`9a9f7f0`](https://github.com/buildzaku/zaku/commit/9a9f7f0dd8c8b55f9e80f5213d24fb7cb870ea77)

- Add option to open existing space from filesystem - [`30b0d57`](https://github.com/buildzaku/zaku/commit/30b0d574597518feab68a17a7d4431423c0e7567)

- Add rust clippy, fmt and test checks in CI workflow - [`269925b`](https://github.com/buildzaku/zaku/commit/269925bf5762a1027fc171fa5b02457fbaef1923)

- Implement dispatch, check & request commands for notification - [`d48f533`](https://github.com/buildzaku/zaku/commit/d48f533c5c9c266f2730f7ee29a242cd60ec86de)

- Add platform specific global shortcuts to toggle devtools - [`89bdb5f`](https://github.com/buildzaku/zaku/commit/89bdb5f5db95a30ced1ffff76c4d4af055c46d73)

- Avoid state reactivity triggers when double-clicking same request - [`4c89295`](https://github.com/buildzaku/zaku/commit/4c8929593ba8b2d8073777ca2f92434c579c9024)
    - Resolves unwanted buffer writes

- Port `move_tree_node` operation logic to rust - [`b36e4e3`](https://github.com/buildzaku/zaku/commit/b36e4e3b0a0598f3ece65809aad44918bb101699)
    - Fix path handling across different platforms

- Improve CI workflow - [`a03b66d`](https://github.com/buildzaku/zaku/commit/a03b66d6be3ca73f65585704c200eea3b8a07131)
    - Add arch targets to the release matrix
    - Enable linting and testing across all platforms

- avoid unnecessary binaries from being included in the release - [`962169c`](https://github.com/buildzaku/zaku/commit/962169ca30c39dcfd773dd5abdfdf0d70255644e)

- Display toast if space is invalid on switch - [`abdcc75`](https://github.com/buildzaku/zaku/commit/abdcc75f8d4c177489919c9e671d88daea02495b)
