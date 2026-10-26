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

### Plugins natifs

Un dossier de plugin qui contient un `plugin.toml` est un plugin natif (prioritaire sur un `*.plugin.zsh` du même dossier) : alias et variables d'environnement y sont déclarés en TOML, les fonctions restent dans des fragments zsh.

```toml
[plugin]
name = "git-extras"          # égal au nom du dossier
version = "0.1.0"
description = "Alias git courants"
init = ["init.zsh"]          # fragments sourcés dans l'ordre
fpath = ["completions"]      # dossiers ajoutés à fpath avant compinit

[aliases]
gst = "git status"

[env]
GIT_EXTRAS_DEBUG = "0"
```

Les variables de tous les plugins sont exportées avant le premier `source`, puis chaque plugin définit ses alias et source ses fragments, dans l'ordre de la configuration. Un manifeste invalide n'empêche pas le shell de démarrer : le plugin est ignoré, avec un commentaire dans le script et un message (fichier et ligne) sur la sortie d'erreur. Le format complet est décrit dans [docs/PLUGIN_TOML.md](docs/PLUGIN_TOML.md) ; des portages natifs de plugins Oh My Zsh courants (`git`, `sudo`, `z`, `extract`, `colored-man-pages`) sont publiés dans le dépôt `maxg56/zsh-tool-plugins`, à copier dans `~/.local/share/zsh-tool/plugins`.

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
