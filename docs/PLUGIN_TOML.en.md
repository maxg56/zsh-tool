# Native plugin format: `plugin.toml`

English | [Français](PLUGIN_TOML.md)

A zsh-tool plugin is a directory. If it contains a `plugin.toml` file, it is a **native** plugin; otherwise, if it contains `<name>.plugin.zsh` (or just a completion function `_<name>`), it is loaded in Oh My Zsh compatibility mode. When both exist in the same directory, `plugin.toml` wins and the `*.plugin.zsh` file is ignored.

This document describes the native format: its fields, the validation rules, and what zsh-tool generates and in which order.

## Example

```
plugins/git-extras/
├── plugin.toml
├── init.zsh
└── completions/
    └── _git-extras
```

```toml
[plugin]
name = "git-extras"
version = "0.1.0"
description = "Common git aliases"
init = ["init.zsh"]          # zsh fragments sourced in order
fpath = ["completions"]      # directories added to fpath

[aliases]
gst = "git status"
glg = "git log --oneline --graph"
"gc!" = "git commit --verbose --amend"

[env]
GIT_EXTRAS_DEBUG = "0"
```

## Fields

| Field | Type | Required | Effect in the generated script |
| --- | --- | --- | --- |
| `plugin.name` | string | yes | Must match the directory name; shown in a comment |
| `plugin.version` | string | no | Metadata (comment, `zsh-tool plugin list`) |
| `plugin.description` | string | no | Metadata (comment, `zsh-tool plugin list`) |
| `plugin.init` | list of paths | no (empty) | `source` of each file, in list order |
| `plugin.fpath` | list of directories | no (empty) | Prepended to `fpath`, before `compinit` |
| `[aliases]` | table of strings | no | One `alias -- name='value'` line per entry |
| `[env]` | table of strings | no | One `export NAME='value'` line per entry |

No other field or table is accepted: a typo (`descripton`, `[alias]`) is an error rather than a setting that is silently ignored.

## Validation rules

- **Syntax**: TOML. Every error gives the file and line, for example `…/plugins/demo/plugin.toml, line 3: unknown field `homepage``.
- **`name`**: not empty, made only of ASCII letters, digits, `.`, `_` and `-`, and equal to the name of the plugin directory (`plugin name `foo` does not match its directory `bar``).
- **`init` and `fpath`**: paths relative to the plugin directory, not empty, not absolute, with no `..` component. Each `init` entry must name an existing file, each `fpath` entry an existing directory.
- **Alias names**: not empty, with no whitespace or control character, and no `=`, `'` or `"`. Other characters are allowed (`gca!`, `...`); a name containing characters that are special to zsh is single-quoted in the script.
- **Variable names**: `[A-Za-z_][A-Za-z0-9_]*`.
- **Values** of aliases and variables: strings only (`DEBUG = 0` is refused; write `DEBUG = "0"`).

Every value written to the script is wrapped in single quotes: `$`, `` ` ``, `;` and quotes are never interpreted when the script is loaded. An alias value is interpreted by zsh when the alias is used, just like an `alias` written by hand.

### Invalid manifest

An invalid manifest does not make `zsh-tool init zsh` fail: the plugin is skipped (no variables, no aliases, no `source`), and the script contains a one-line comment

```zsh
# plugin demo: invalid manifest: /…/plugins/demo/plugin.toml, line 3: init file `init.zsh` does not exist
```

The same message is printed on standard error at every shell start until the manifest is fixed. The lookup does not move on to the next directory: a directory that contains `plugin.toml` is always the plugin's directory, even when the manifest is invalid.

## Resolution

A plugin name is looked up in `~/.local/share/zsh-tool/plugins` (or `$ZSH_TOOL_HOME/plugins`), then `$ZSH_CUSTOM/plugins`, then `$ZSH/plugins`. The first directory that contains a plugin (native or Oh My Zsh) is used.

## Generation order

For a configuration `plugins = ["a", "b", "c"]`, the `init.zsh` script contains, in this order:

1. The Oh My Zsh compatibility variables (`ZSH`, `ZSH_CUSTOM`, `ZSH_CACHE_DIR`).
2. The `[env]` tables of **all** native plugins, plugin by plugin in configuration order, so a fragment can read a variable declared by a plugin listed after it.
3. `fpath`: the `fpath` directories of native plugins and the directories of Oh My Zsh plugins, in configuration order.
4. For each plugin, in configuration order: its declared aliases, then a `source` of each of its `init` files. A fragment can therefore redefine an alias declared in its own manifest.
5. `compinit`, once; `compdef` calls made by fragments before `compinit` are replayed afterwards.

Within `[aliases]` and `[env]`, the order of the file is kept, which makes the script deterministic.

Output for the manifest above:

```zsh
# environment
export GIT_EXTRAS_DEBUG='0'

# fpath
typeset -gU fpath
fpath=('/…/plugins/git-extras/completions' "$ZSH_CACHE_DIR/completions" $fpath)

# plugins
# plugin git-extras 0.1.0: Common git aliases
alias -- gst='git status'
alias -- glg='git log --oneline --graph'
alias -- 'gc!'='git commit --verbose --amend'
source '/…/plugins/git-extras/init.zsh'
```

## Cache

The script is rebuilt when the modification time of `plugin.toml`, of the plugin directory, of an `init` file or of an `fpath` directory changes, or when the validation result changes (for example when a missing `init` file is created).

## Writing `init` fragments

- Simple aliases belong in `[aliases]`; functions, ZLE widgets, key bindings and conditional aliases (depending on a tool version, for instance) stay in zsh fragments.
- Fragments are loaded with `source`, so `${(%):-%N}` or `$0` refers to the sourced file, which lets you find the plugin directory.
- `compdef` can be called before `compinit`: the call is recorded and replayed.
- Completion functions `_name` go in a directory declared in `fpath` (`completions/` by convention).

## Differences from Oh My Zsh plugins

| | Oh My Zsh plugin (`*.plugin.zsh`) | Native plugin (`plugin.toml`) |
| --- | --- | --- |
| Detection | `<name>.plugin.zsh` or `_<name>` | `plugin.toml` (takes precedence) |
| Metadata | none | `name`, `version`, `description` |
| Aliases | `alias` lines in the script | `[aliases]` table, values quoted by zsh-tool |
| Variables | `export` lines in the script | `[env]` table, set before any plugin |
| `fpath` | the whole plugin directory is added | only the declared directories |
| Code | a single sourced file | ordered list of `init` fragments (possibly empty) |
| Dependencies | often assumes Oh My Zsh's `lib/` (`git_current_branch`, `clipcopy`, colours…) | self-contained: defines its own helper functions |
| Errors | a zsh syntax error shows up at start-up | the manifest is validated by zsh-tool (file and line); the plugin is skipped if it is invalid |
