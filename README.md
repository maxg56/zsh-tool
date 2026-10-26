# zsh-tool

Gestionnaire de configuration zsh écrit en Rust, compatible avec les plugins Oh My Zsh, qui génère et met en cache le script d'initialisation du shell. Voir le [cahier des charges](docs/CAHIER_DES_CHARGES.md).

## Utilisation

Compiler et installer :

```sh
cargo install --path .
```

Créer `~/.config/zsh-tool/config.toml` :

```toml
plugins = ["git", "sudo", "z"]

[prompt]
engine = "none"   # "none", "starship" ou "legacy"
theme = "robbyrussell"

[compat]
lib = false       # true : charge aussi les fichiers lib/ d'Oh My Zsh
```

Puis, dans `~/.zshrc` :

```sh
eval "$(zsh-tool init zsh)"
```

Depuis Oh My Zsh, `zsh-tool migrate` écrit ce fichier à partir du `.zshrc` (plugins, thème) et y remplace `source $ZSH/oh-my-zsh.sh` par la ligne ci-dessus, après une sauvegarde ; `zsh-tool migrate --dry-run` montre le résultat sans rien écrire. Voir le [guide de migration](docs/MIGRATION.md).

Les plugins sont cherchés dans cet ordre : `~/.local/share/zsh-tool/plugins`, `$ZSH_CUSTOM/plugins`, puis `$ZSH/plugins` (Oh My Zsh, par défaut `~/.oh-my-zsh`). Un plugin introuvable est signalé par un commentaire dans le script, sans bloquer le shell.

Moteurs de prompt :

- `none` : zsh-tool ne touche pas au prompt ;
- `starship` : le script lance `eval "$(starship init zsh)"` si `starship` est dans le `PATH`, sinon il contient seulement un commentaire ;
- `legacy` : charge le thème Oh My Zsh `<theme>.zsh-theme`, cherché dans `~/.local/share/zsh-tool/themes`, `$ZSH_CUSTOM/themes`, `$ZSH_CUSTOM` puis `$ZSH/themes`, après les plugins et `compinit`. zsh-tool fournit lui-même les fonctions utilisées par les thèmes (`git_prompt_info`, `parse_git_dirty`, `git_prompt_status`, couleurs...), sans dépendre de `lib/` d'Oh My Zsh. Un thème introuvable est signalé sur la sortie d'erreur, sans bloquer le shell.

Avec `compat.lib = true`, les fichiers `$ZSH/lib/*.zsh` d'Oh My Zsh sont chargés avant les plugins, comme le fait Oh My Zsh.

Le script est mis en cache dans `~/.cache/zsh-tool/init.zsh` et régénéré quand la configuration, un plugin, le thème, le binaire `starship`, un fichier `lib/` chargé ou la version de zsh-tool change. `compinit` n'est lancé qu'une fois, après l'ajout de tous les `fpath`, avec un fichier de cache dans le même dossier.

| Variable | Rôle |
| --- | --- |
| `ZSH_TOOL_CONFIG` | Chemin de `config.toml` |
| `ZSH_TOOL_HOME` | Dossier des plugins et thèmes de zsh-tool |
| `ZSH`, `ZSH_CUSTOM`, `ZSH_CACHE_DIR` | Emplacements Oh My Zsh, transmis aux plugins |

## Développement

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                       # les tests d'intégration utilisent zsh s'il est installé
ZSH_TOOL_REQUIRE_ZSH=1 cargo test  # échoue si zsh est absent (CI)
```

Les tests de migration et de thèmes utilisent un extrait d'Oh My Zsh copié dans `tests/fixtures/ohmyzsh` (licence MIT) et demandent `git`.

Les mesures de démarrage et la façon de les reproduire sont dans [docs/MESURES.md](docs/MESURES.md).
