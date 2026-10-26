# zsh-tool

English | [Français](README.fr.md)

A zsh configuration manager written in Rust that works with Oh My Zsh plugins. zsh-tool reads a `config.toml` file, generates the shell's init script and caches it, so your shell starts faster with the same plugins and theme.

- Existing Oh My Zsh plugins and themes work as they are; `zsh-tool migrate` converts an Oh My Zsh `.zshrc` in one command.
- Native plugins, described by a `plugin.toml`, declare aliases and variables in TOML and are validated before the shell starts.
- Plugins are installed from the official registry, GitHub or any HTTPS URL, and pinned to an exact commit in `zsh-tool.lock`.
- A single static binary with a built-in Git client.

Runs on Linux and macOS (x86_64 and arm64) with zsh 5.8 or later. The project's goals are set out in the [specification](docs/CAHIER_DES_CHARGES.md) (in French).

## Installation

### Release binaries

Each [GitHub release](https://github.com/maxg56/zsh-tool/releases) ships one archive per platform, with its SHA-256 checksum:

| Platform | Target |
| --- | --- |
| Linux x86_64 (static binary) | `x86_64-unknown-linux-musl` |
| Linux arm64 (static binary) | `aarch64-unknown-linux-musl` |
| macOS Intel | `x86_64-apple-darwin` |
| macOS Apple Silicon | `aarch64-apple-darwin` |

```sh
version=v0.1.0
target=x86_64-unknown-linux-musl
base=https://github.com/maxg56/zsh-tool/releases/download/$version
curl -LO "$base/zsh-tool-$version-$target.tar.gz"
curl -LO "$base/zsh-tool-$version-$target.tar.gz.sha256"
shasum -a 256 -c "zsh-tool-$version-$target.tar.gz.sha256"
tar -xzf "zsh-tool-$version-$target.tar.gz"
install -m 755 "zsh-tool-$version-$target/zsh-tool" ~/.local/bin/
```

The archive holds the `zsh-tool` binary, `LICENSE` and this README. The install directory (`~/.local/bin` here) must be on your `PATH`.

### Homebrew

Each release comes with a Homebrew formula, `zsh-tool.rb`, which installs that release's binary for the current platform (and depends on `git` and `zsh`). To install it directly:

```sh
curl -LO https://github.com/maxg56/zsh-tool/releases/download/v0.1.0/zsh-tool.rb
brew install --formula ./zsh-tool.rb
```

Or put it in a personal tap, so that `brew upgrade` picks up a newer formula later:

```sh
brew tap-new "$USER/local"
cp zsh-tool.rb "$(brew --repository "$USER/local")/Formula/"
brew install "$USER/local/zsh-tool"
```

### With Cargo

```sh
cargo install zsh-tool
```

This needs Rust 1.88 or later and installs the binary in `~/.cargo/bin`.

### From source

```sh
git clone https://github.com/maxg56/zsh-tool.git
cd zsh-tool
cargo install --locked --path .
```

## Quick start

`zsh-tool` must be on your `PATH` when `.zshrc` runs (for example, add `~/.cargo/bin` or `~/.local/bin` to `PATH` in `~/.zshenv`).

### Coming from Oh My Zsh

```sh
zsh-tool migrate --dry-run   # show config.toml and the .zshrc changes, write nothing
zsh-tool migrate             # write config.toml and update .zshrc (after a backup)
exec zsh
```

`migrate` reads the plugins and theme from `.zshrc`, writes `~/.config/zsh-tool/config.toml`, then replaces `source $ZSH/oh-my-zsh.sh` with `eval "$(zsh-tool init zsh)"`. The original `.zshrc` is copied to `.zshrc.zsh-tool.bak`. Plugins and themes are still loaded from the Oh My Zsh directory, so keep it. The [migration guide](docs/MIGRATION.en.md) covers what is read, what changes, how to roll back and the known limitations.

### Manual setup

Create `~/.config/zsh-tool/config.toml`:

```toml
plugins = ["git", "sudo", "z"]
```

Then add this line to `~/.zshrc`:

```sh
eval "$(zsh-tool init zsh)"
```

Run `zsh-tool doctor` to check the setup.

## Configuration

`~/.config/zsh-tool/config.toml` (or `$XDG_CONFIG_HOME/zsh-tool/config.toml`, or `$ZSH_TOOL_CONFIG`):

```toml
plugins = ["git", "sudo", "z"]

[prompt]
engine = "none"   # "none", "starship" or "legacy"
theme = "robbyrussell"

[compat]
lib = false       # true: also load Oh My Zsh's lib/ files
```

Every field is optional, and a missing file is the same as an empty configuration. Unknown fields and syntax errors are reported with their line number.

Plugin names may only contain letters, digits, `.`, `_` and `-`; they cannot start with `.` or `-` or contain `..`. An invalid name in `config.toml` is an error, reported with its line number.

Plugins are looked up in this order: `~/.local/share/zsh-tool/plugins`, `$ZSH_CUSTOM/plugins`, then `$ZSH/plugins` (Oh My Zsh, `~/.oh-my-zsh` by default). A plugin that cannot be found leaves a comment in the script and does not stop the shell from starting.

With `compat.lib = true`, Oh My Zsh's `$ZSH/lib/*.zsh` files are loaded before the plugins, as Oh My Zsh itself does. This makes start-up slower.

### Cache

The script is cached in `~/.cache/zsh-tool/init.zsh` and rebuilt whenever the configuration, a plugin, the theme, the `starship` binary, a loaded `lib/` file or the zsh-tool version changes, so you never need to clear it by hand. `compinit` runs only once, after every `fpath` entry has been added, with its dump file in the same directory; the dump is compiled with `zcompile` on first start so that it loads faster.

## Prompt engines

- `none` (default): zsh-tool leaves the prompt alone.
- `starship`: the script runs `eval "$(starship init zsh)"` if `starship` is on the `PATH`; otherwise it only contains a comment.
- `legacy`: loads the Oh My Zsh theme `<theme>.zsh-theme`, looked up in `~/.local/share/zsh-tool/themes`, `$ZSH_CUSTOM/themes`, `$ZSH_CUSTOM` then `$ZSH/themes`, after the plugins and `compinit`. zsh-tool provides the functions themes rely on (`git_prompt_info`, `parse_git_dirty`, `git_prompt_status`, colours...) itself, without depending on Oh My Zsh's `lib/`. A missing theme is reported on standard error and does not stop the shell from starting.

The full list of functions provided to themes is in the [migration guide](docs/MIGRATION.en.md#legacy-themes-and-lib).

## Managing plugins

```sh
zsh-tool plugin add git                              # already installed (zsh-tool, ZSH_CUSTOM, Oh My Zsh): just enabled
zsh-tool plugin add zsh-users/zsh-autosuggestions    # GitHub repository, cloned over HTTPS
zsh-tool plugin add https://example.com/me/plug.git --name plug
zsh-tool plugin list                                 # name, origin, source and commit
zsh-tool plugin update [name…]                       # every managed plugin by default
zsh-tool plugin remove plug
```

`plugin add` accepts:

- a name: if the plugin is already in one of the search directories, it is only added to `config.toml`; otherwise it is copied from the official registry (`plugins/<name>/` in `https://github.com/maxg56/zsh-tool-plugins.git`, cloned into `~/.local/share/zsh-tool/registry`);
- `owner/repo` (GitHub) or an `https://` URL: the repository is cloned into `~/.local/share/zsh-tool/plugins/<name>`, where the name is the last segment of the URL without `.git` (use `--name` to change it);
- an absolute path to a local Git repository, to try out a plugin you are writing.

Remote repositories must use HTTPS: `http://`, `git://`, `ssh://` and `git@…` are refused. The plugin is appended to `plugins` in `config.toml` (created if needed), keeping its comments and formatting; running the command again changes nothing. No plugin code runs until the shell loads it.

`plugin remove` takes the plugin out of `config.toml` and deletes its directory only if zsh-tool installed it: nothing is ever deleted from Oh My Zsh or `ZSH_CUSTOM`. `plugin update` will not touch a plugin whose directory has local changes (a file modified, added or deleted).

The Git client is built in (gix, with TLS through rustls), so the `git` command is not needed for HTTPS repositories; only local paths go through `git-upload-pack`. The proxy (`HTTPS_PROXY`) and certificate authority (`http.sslCAInfo`, `GIT_SSL_CAINFO`) from your Git configuration are honoured.

## Lock file and updates

Every plugin installed by zsh-tool is recorded in `zsh-tool.lock`, next to `config.toml`, with its source (a URL or `registry`) and exact commit, sorted by name:

```toml
[[plugin]]
name = "zsh-autosuggestions"
source = "https://github.com/zsh-users/zsh-autosuggestions.git"
commit = "85919cd1ffa7d2d5412f6d3fe437ebdbeeec4fc5"
```

```sh
zsh-tool upgrade   # update the registry and every managed plugin, printing old -> new commit
zsh-tool install   # reinstall missing plugins at the locked commits (on another machine, for instance)
```

To reproduce your setup on another machine, copy `config.toml` and `zsh-tool.lock` there and run `zsh-tool install`. There is no need to clear the cache after these commands: it is rebuilt as soon as `config.toml` or a plugin changes.

## Diagnostics

```sh
zsh-tool doctor
```

`doctor` checks, without network access: the configuration (with the line of any error), that every plugin can be found, that managed plugins are present and match the locked commit, that zsh 5.8 or later is on the `PATH`, that Starship is installed when `engine = "starship"`, whether Oh My Zsh is present, and that the cache is writable. Each line is tagged `ok`, `info`, `warning` or `error`; the exit code is non-zero if there is at least one error.

## Native plugins

A plugin directory that contains a `plugin.toml` is a native plugin (it takes precedence over a `*.plugin.zsh` in the same directory). Aliases and environment variables are declared in TOML; functions stay in zsh fragments.

```toml
[plugin]
name = "git-extras"          # must match the directory name
version = "0.1.0"
description = "Common git aliases"
init = ["init.zsh"]          # fragments sourced in order
fpath = ["completions"]      # directories added to fpath before compinit

[aliases]
gst = "git status"

[env]
GIT_EXTRAS_DEBUG = "0"
```

The variables of all plugins are exported before the first `source`; then each plugin defines its aliases and sources its fragments, in configuration order. An invalid manifest does not stop the shell from starting: the plugin is skipped, with a comment in the script and a message (file and line) on standard error. The full format is described in the [plugin.toml specification](docs/PLUGIN_TOML.en.md).

Native ports of the 20 reference Oh My Zsh plugins (`git`, `sudo`, `z`, `extract`, `colored-man-pages`, `history`, `docker`, `docker-compose`, `kubectl`, `npm`, `node`, `nvm`, `python`, `pip`, `pyenv`, `fzf`, `command-not-found`, `web-search`, `copypath`, `dirhistory`) are published in [maxg56/zsh-tool-plugins](https://github.com/maxg56/zsh-tool-plugins), the default registry for `zsh-tool plugin add <name>`. They do not depend on Oh My Zsh's `lib/`. The `nvm` and `pyenv` ports load their tool on first use (`nvm`, `node`, `npm`… or `pyenv`, `python`, `pip`…); `zstyle ':omz:plugins:nvm' lazy no` (or `':omz:plugins:pyenv'`), placed before `eval "$(zsh-tool init zsh)"`, loads it at start-up instead.

## Environment variables

| Variable | Purpose |
| --- | --- |
| `ZSH_TOOL_CONFIG` | Path to `config.toml` (and therefore to `zsh-tool.lock`) |
| `ZSH_TOOL_HOME` | Directory for zsh-tool's plugins, registry and themes (`~/.local/share/zsh-tool` by default) |
| `ZSH_TOOL_REGISTRY` | Registry repository (an `https://` URL or an absolute local path) |
| `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME` | Replace `~/.config`, `~/.local/share` and `~/.cache` in the default paths |
| `ZSH`, `ZSH_CUSTOM`, `ZSH_CACHE_DIR` | Oh My Zsh locations, passed on to plugins |

## Performance

Milestone J5 measurements on the reference machine (Linux x86_64 container, 4 vCPUs, zsh 5.9), timing `zsh -i -c exit` with hyperfine over 50 runs:

| Setup | 10 plugins | 20 plugins |
| --- | ---: | ---: |
| Oh My Zsh | 63.0 ± 6.7 ms | 75.8 ± 9.5 ms |
| zsh-tool, Oh My Zsh plugins | 37.3 ± 4.1 ms | 44.4 ± 5.2 ms |
| zsh-tool, native plugins | 35.7 ± 4.7 ms | 44.4 ± 4.3 ms |
| `zsh-tool init zsh` alone (valid cache) | 3.8 ± 0.5 ms | 4.0 ± 1.0 ms |

Of the 20 reference plugins, 19 load without errors, under zsh-tool and Oh My Zsh alike; the twentieth, `fzf`, deliberately prints an error when fzf is not installed. The method, the machine, the plugin list and how to reproduce the numbers are in [docs/MESURES.md](docs/MESURES.md) (in French).

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                         # integration tests use zsh when installed, and git to set up local repositories
ZSH_TOOL_REQUIRE_ZSH=1 cargo test  # fails if zsh is missing (CI)
```

The migration and theme tests use a subset of Oh My Zsh copied into `tests/fixtures/ohmyzsh` (MIT licence) and need `git`. CI (`.github/workflows/ci.yml`) runs these commands on Linux and macOS and checks that the crate builds with the minimum supported Rust version. The release procedure is described in [docs/RELEASE.md](docs/RELEASE.md) (in French).

Start-up benchmarks ([docs/MESURES.md](docs/MESURES.md)):

```sh
scripts/bench.sh --native ../zsh-tool-plugins/plugins /tmp/omz           # timings, 10 plugins
scripts/bench.sh --native ../zsh-tool-plugins/plugins --set 20 /tmp/omz  # timings, 20 plugins
scripts/bench.sh --check --native ../zsh-tool-plugins/plugins /tmp/omz   # error-free loading, X/20
```

## Documentation

- [Migrating from Oh My Zsh](docs/MIGRATION.en.md)
- [plugin.toml format specification](docs/PLUGIN_TOML.en.md)
- [Start-up measurements](docs/MESURES.md) (in French)
- [Releasing a version](docs/RELEASE.md) (in French)
- [Specification](docs/CAHIER_DES_CHARGES.md) (in French)

## License

MIT, see [LICENSE](LICENSE).
