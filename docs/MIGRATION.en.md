# Migrating from Oh My Zsh

English | [Français](MIGRATION.md)

`zsh-tool migrate` converts an Oh My Zsh `.zshrc`: it writes a `config.toml` with the same plugins and theme, then replaces the Oh My Zsh loader with `zsh-tool init zsh`. Plugins and themes are still taken from the Oh My Zsh directory, so keep it.

## In short

```sh
zsh-tool migrate --dry-run   # show config.toml and the .zshrc changes, write nothing
zsh-tool migrate             # run the migration
exec zsh                     # start a new shell
```

| Option | Effect |
| --- | --- |
| `--zshrc <path>` | File to migrate (default: `$ZDOTDIR/.zshrc`, otherwise `~/.zshrc`) |
| `--dry-run` | Print the planned `config.toml` and a diff of `.zshrc`, without writing anything |
| `--force` | Overwrite an existing `config.toml` (refused otherwise) |

`zsh-tool` must be on the `PATH` when `.zshrc` runs (for example, add `~/.cargo/bin` in `~/.zshenv`).

## What is read

- `plugins=(...)`, on one or several lines, with comments. Commented-out lines are ignored, `plugins+=(...)` appends to the list, and the last assignment wins. A name that cannot be read (`$var`, `$(...)`) or that contains anything other than letters, digits, `.`, `_` and `-` is skipped with a warning.
- `ZSH_THEME`, in double quotes, single quotes or unquoted.
- `ZSH` and `ZSH_CUSTOM` (with or without `export`), with `~`, `$HOME` and `$ZSH` expanded; they are used to check that the plugins and the theme exist.
- An `eval "$(starship init zsh)"` line.

## Which prompt engine is chosen

| `.zshrc` | `[prompt]` |
| --- | --- |
| Contains `eval "$(starship init zsh)"` | `engine = "starship"` |
| `ZSH_THEME=""` or no `ZSH_THEME` | `engine = "none"` |
| `ZSH_THEME="random"` | `engine = "legacy"`, `theme = "robbyrussell"` (with a warning) |
| `ZSH_THEME="<name>"` | `engine = "legacy"`, `theme = "<name>"` |

A plugin or theme that cannot be found triggers a warning but stays in the configuration: it will be loaded as soon as it is installed, and in the meantime the shell starts normally (the script contains a comment in its place).

## What changes in `.zshrc`

Before any change, the original file is copied to `.zshrc.zsh-tool.bak` (or `.zshrc.zsh-tool.<date>.bak` if that backup already exists; a backup is never overwritten). Then:

- the `source $ZSH/oh-my-zsh.sh` line (or `. ...`, `${ZSH}`, a literal path, quotes) becomes `eval "$(zsh-tool init zsh)"`;
- the `plugins=(...)` and `ZSH_THEME=...` lines, and the `eval "$(starship init zsh)"` line with the Starship engine, are commented out with the prefix `# zsh-tool: `;
- `ZSH=...` and `ZSH_CUSTOM=...` become `export ZSH=...` if they were not exported, because `zsh-tool init zsh` reads them from its environment;
- everything else is kept as is. A `.zshrc` that is a symbolic link (from a dotfiles manager) stays a link: its target is rewritten.

The migration refuses a `.zshrc` without a recognised `source .../oh-my-zsh.sh` line, or one that already calls `zsh-tool init zsh`; nothing is written in that case.

For example, with the `.zshrc` from the Oh My Zsh template:

```diff
-ZSH_THEME="robbyrussell"
+# zsh-tool: ZSH_THEME="robbyrussell"
-plugins=(git sudo z)
+# zsh-tool: plugins=(git sudo z)
-source $ZSH/oh-my-zsh.sh
+eval "$(zsh-tool init zsh)"
```

## Legacy themes and `lib/`

With `engine = "legacy"`, zsh-tool loads the `<theme>.zsh-theme` file, looked up in this order: `~/.local/share/zsh-tool/themes`, `$ZSH_CUSTOM/themes`, `$ZSH_CUSTOM`, `$ZSH/themes`. The script turns on `prompt_subst` and colours (`$fg`, `$fg_bold`, `$reset_color`, and `$FG`/`$BG`/`$FX` if the theme uses them), then provides its own version of the Oh My Zsh functions that themes depend on, without loading `lib/`:

- git: `git_prompt_info`, `parse_git_dirty`, `git_prompt_status`, `git_current_branch`, `git_remote_status`, `git_prompt_ahead`, `git_prompt_behind`, `git_prompt_remote`, `git_commits_ahead`, `git_commits_behind`, `git_prompt_short_sha`, `git_prompt_long_sha`, `git_current_user_name`, `git_current_user_email`, `git_repo_name`, `git_previous_branch`. The `ZSH_THEME_GIT_PROMPT_*`, `ZSH_THEME_GIT_SHOW_UPSTREAM`, `DISABLE_UNTRACKED_FILES_DIRTY` and `GIT_STATUS_IGNORE_SUBMODULES` settings and the git options `oh-my-zsh.hide-info`, `oh-my-zsh.hide-status` and `oh-my-zsh.hide-dirty` are honoured;
- `ruby_prompt_info`, `rvm_prompt_info`, and empty functions (`virtualenv_prompt_info`, `nvm_prompt_info`, `hg_prompt_info`, etc.) that the matching plugins replace;
- coloured `ls` and `diff`, `LSCOLORS`, `LS_COLORS` (unless `DISABLE_LS_COLORS=true`), `ZSH_THEME_TERM_TITLE_IDLE`.

These functions are synchronous (there is no asynchronous prompt yet). They are defined before the plugins, so a plugin that ships its own version replaces them.

The other settings from `lib/` (history, completion, key bindings, terminal title, `take`, `omz`...) are not applied by default. To get exactly the Oh My Zsh behaviour back, turn on:

```toml
[compat]
lib = true
```

All `$ZSH/lib/*.zsh` files are then loaded in alphabetical order, before the plugins, as Oh My Zsh does (a file with the same name in `$ZSH_CUSTOM/lib` replaces it). They override zsh-tool's theme functions with Oh My Zsh's, including the asynchronous git prompt. This option makes start-up slower.

## Rolling back

```sh
mv ~/.zshrc.zsh-tool.bak ~/.zshrc
```

You can delete `~/.config/zsh-tool/config.toml` or keep it: Oh My Zsh ignores it.

## Known limitations

- The `$ZSH_CUSTOM/*.zsh` files, which Oh My Zsh loads after the plugins, are not loaded: source them from `.zshrc` if you need them.
- `zstyle ':omz:...'` settings (turning off a plugin's aliases, updates) and Oh My Zsh's automatic updates no longer apply.
- Only simple forms are recognised: one assignment or one `source` per line. A `plugins=(...)` built dynamically, or a `source` combined with other commands on the same line, has to be adapted by hand.
