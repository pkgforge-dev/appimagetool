
## [0.1.0] - 2026-05-03

### ⛰️  Features

- Add --verbose and --quiet flags with structured logging - ([4b3e7e5](https://github.com/pkgforge-dev/appimagetool/commit/4b3e7e52a4b9098b6473cac15c744bc7ba041ab1))
- Close remaining gaps with quick-sharun.sh - ([8d6ea43](https://github.com/pkgforge-dev/appimagetool/commit/8d6ea431f576d90d7c6e664f305216e880dbc58c))
- Use zsync-rs library instead of shelling out to zsyncmake - ([8b105af](https://github.com/pkgforge-dev/appimagetool/commit/8b105af3b9345825dba935251662d3598fe989be))
- Handle DEVEL_RELEASE UPINFO rewrite and decouple configure_runtime from Config - ([9fdc589](https://github.com/pkgforge-dev/appimagetool/commit/9fdc589ea7f822aaeb1b92ec6fd14d74a28fd8c3))
- Add appimage pipeline and CLI entry point - ([57e18fa](https://github.com/pkgforge-dev/appimagetool/commit/57e18fa638e316ea6c817cfc4b5adb907fc0eb0f))
- Add dwarfs module for building DWARFS images and profiling - ([0a7d7b4](https://github.com/pkgforge-dev/appimagetool/commit/0a7d7b42b15d87df2ae7ab563126a6a12314acc9))
- Add desktop module for parsing .desktop files and computing output name - ([705a4dd](https://github.com/pkgforge-dev/appimagetool/commit/705a4dd9051dd15feb20777c476fab16a89ab19a))
- Add uruntime module for downloading and configuring runtime - ([fa603f9](https://github.com/pkgforge-dev/appimagetool/commit/fa603f9f6fa5c8280c95ce80ccc1b6a6c6a395b9))
- Add util module with download, sanitize_filename, is_elf helpers - ([2de4937](https://github.com/pkgforge-dev/appimagetool/commit/2de49372cddb5d6cd5495acb5e5fc1915dcfdc0c))
- Add project skeleton with elf section editor, config, error types - ([3b27373](https://github.com/pkgforge-dev/appimagetool/commit/3b273733ef7d013be953ce9c241de6ebedabcd61))

### 🐛 Bug Fixes

- *(desktop)* Preserve whitespace in parsed .desktop values - ([fe65b18](https://github.com/pkgforge-dev/appimagetool/commit/fe65b18eac8f4c7fd5393cfe6708a3f53f8f43ca))
- *(dwarfs)* Skip .mount_*.pid sidecars when snapshotting mounts - ([90f9ccb](https://github.com/pkgforge-dev/appimagetool/commit/90f9ccbd39677b704857561e06326d693aa61e06))
- *(dwarfs)* Require executable bit when resolving binaries on PATH - ([b2c2ff6](https://github.com/pkgforge-dev/appimagetool/commit/b2c2ff68a924e5a340091b4f5ea17a0f634be15d))
- *(dwarfs)* Scope profile cleanup and harden process teardown - ([5958b3f](https://github.com/pkgforge-dev/appimagetool/commit/5958b3fc7970972099531278b971d27483cc833d))
- *(elf)* Bounds-check parsing and surface patch outcomes - ([9c0cd46](https://github.com/pkgforge-dev/appimagetool/commit/9c0cd4669b73fbdfed7c5d335bc9b0721ac003c3))
- *(util)* Strip path separators and NUL from sanitized filenames - ([2861eb8](https://github.com/pkgforge-dev/appimagetool/commit/2861eb8cfd2c60600b0af6496a49be7e85200c5f))
- Make tmpdir paths per-process and downloads atomic - ([cf2e6c3](https://github.com/pkgforge-dev/appimagetool/commit/cf2e6c3de358fdc071ef1143697554b385870689))
- Accept truthy values for OPTIMIZE_LAUNCH and other bool envs - ([3f73d0b](https://github.com/pkgforge-dev/appimagetool/commit/3f73d0b314874cc48e6da714683abbe6036a199e))
- Copy cached runtime before patching to keep cache pristine - ([ae7e8bb](https://github.com/pkgforge-dev/appimagetool/commit/ae7e8bb474e3355ddfdcf43aea1e10694fdc6b9a))
- Split DWARFS compression string correctly, first arg to -C rest as separate args - ([16fdac5](https://github.com/pkgforge-dev/appimagetool/commit/16fdac5a997ccd7812a2bf5708d4a49050b4aa64))

### 🚜 Refactor

- Replace Config::from_cli_args positional args with CliArgs struct - ([d693786](https://github.com/pkgforge-dev/appimagetool/commit/d693786221f6eb8e8eb9103af6ee7a82b33a44c7))
- Dedupe binary download/cache logic into util - ([97b1af7](https://github.com/pkgforge-dev/appimagetool/commit/97b1af72b891e170207812e480c43fb091bbb972))
- Add actionable hints to error messages - ([06ab6ce](https://github.com/pkgforge-dev/appimagetool/commit/06ab6ce8e6ef0cdbfaf742a617af62dc81045488))
- Remove redundant env var reads now that clap handles them - ([5f6999e](https://github.com/pkgforge-dev/appimagetool/commit/5f6999ec13c0fd999b10670d6194ba49e0c2d4bc))
- Remove squashfs compression option, DWARFS only for now - ([dfe5b6a](https://github.com/pkgforge-dev/appimagetool/commit/dfe5b6a62ed1d4c8a36645c0f1b6b2ec5bdb9881))

### 📚 Documentation

- Rustdoc public APIs and enforce missing_docs at the crate root - ([93ee20b](https://github.com/pkgforge-dev/appimagetool/commit/93ee20b475543b220cac7f82697d2a6d0ca32a96))
- Update README with --verbose/--quiet flags and log module - ([bad9deb](https://github.com/pkgforge-dev/appimagetool/commit/bad9deb64bcce0f84486dee83f07b7abb19ef774))
- Add README with usage, options, env vars, and architecture overview - ([b4901cb](https://github.com/pkgforge-dev/appimagetool/commit/b4901cb8115c32cac3da30fb9530b929efa9b67d))

### 🧪 Testing

- *(log)* Serialize tests that mutate the global LEVEL - ([4f4b070](https://github.com/pkgforge-dev/appimagetool/commit/4f4b07036df2428ab73c6b954e76b799d5845645))
- Drop sort_env_file integration test that re-implemented the logic - ([5c2b62e](https://github.com/pkgforge-dev/appimagetool/commit/5c2b62e2d59192b6024ad965ddb698cdf48f9dc1))
- Add unit tests for config, desktop, util, and appimage modules - ([f9eb2de](https://github.com/pkgforge-dev/appimagetool/commit/f9eb2defdcf19842cad82cd7f8890957d8558703))
- Add integration tests for desktop, config, util, and error modules - ([800c68d](https://github.com/pkgforge-dev/appimagetool/commit/800c68d044aa5534df7ac6d083888927a501a477))

### ⚙️ Miscellaneous Tasks

- Add release workflow - ([3d11e55](https://github.com/pkgforge-dev/appimagetool/commit/3d11e55014cb0f744d71726bc0b4c9e35d43242b))
