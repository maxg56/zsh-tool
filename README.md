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
```

Puis, dans `~/.zshrc` :

```sh
eval "$(zsh-tool init zsh)"
```

Les plugins sont cherchés dans cet ordre : `~/.local/share/zsh-tool/plugins`, `$ZSH_CUSTOM/plugins`, puis `$ZSH/plugins` (Oh My Zsh, par défaut `~/.oh-my-zsh`). Un plugin introuvable est signalé par un commentaire dans le script, sans bloquer le shell.

Le script est mis en cache dans `~/.cache/zsh-tool/init.zsh` et régénéré quand la configuration, un plugin ou la version de zsh-tool change. `compinit` n'est lancé qu'une fois, après l'ajout de tous les `fpath`, avec un fichier de cache dans le même dossier.

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

Les mesures de démarrage et la façon de les reproduire sont dans [docs/MESURES.md](docs/MESURES.md).
