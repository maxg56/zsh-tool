# zsh-tool

[English](README.md) | Français

Gestionnaire de configuration zsh écrit en Rust, compatible avec les plugins Oh My Zsh. zsh-tool lit un fichier `config.toml`, génère le script d'initialisation du shell et le met en cache : le shell démarre plus vite, sans changer de plugins ni de thème.

- Les plugins et thèmes Oh My Zsh existants fonctionnent tels quels ; `zsh-tool migrate` convertit un `.zshrc` Oh My Zsh en une commande.
- Des plugins natifs, décrits par un `plugin.toml`, déclarent alias et variables en TOML et se valident avant le démarrage du shell.
- Les plugins sont installés depuis le registre officiel, GitHub ou une URL HTTPS, et verrouillés à un commit exact dans `zsh-tool.lock`.
- Un seul binaire statique, avec un client Git intégré.

Linux et macOS (x86_64 et arm64), zsh 5.8 ou plus. Le [cahier des charges](docs/CAHIER_DES_CHARGES.md) décrit les objectifs du projet.

## Installation

### Binaires de release

Chaque [release GitHub](https://github.com/maxg56/zsh-tool/releases) fournit une archive par plateforme, avec sa somme SHA-256 :

| Plateforme | Cible |
| --- | --- |
| Linux x86_64 (binaire statique) | `x86_64-unknown-linux-musl` |
| Linux arm64 (binaire statique) | `aarch64-unknown-linux-musl` |
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

L'archive contient le binaire `zsh-tool`, `LICENSE` et le README (en anglais). Le dossier d'installation (ici `~/.local/bin`) doit être dans le `PATH`.

### Homebrew

Chaque release joint une formule Homebrew, `zsh-tool.rb`, qui installe le binaire de la release pour la plateforme courante (et dépend de `git` et `zsh`). Pour l'installer directement :

```sh
curl -LO https://github.com/maxg56/zsh-tool/releases/download/v0.1.0/zsh-tool.rb
brew install --formula ./zsh-tool.rb
```

Ou dans un tap personnel, ce qui permet ensuite `brew upgrade` avec une formule plus récente :

```sh
brew tap-new "$USER/local"
cp zsh-tool.rb "$(brew --repository "$USER/local")/Formula/"
brew install "$USER/local/zsh-tool"
```

### Avec Cargo

```sh
cargo install zsh-tool
```

Demande Rust 1.88 ou plus. Le binaire est installé dans `~/.cargo/bin`.

### Depuis les sources

```sh
git clone https://github.com/maxg56/zsh-tool.git
cd zsh-tool
cargo install --locked --path .
```

## Démarrage rapide

`zsh-tool` doit être dans le `PATH` au moment où le `.zshrc` s'exécute (par exemple `~/.cargo/bin` ou `~/.local/bin` ajouté dans `~/.zshenv`).

### Depuis Oh My Zsh

```sh
zsh-tool migrate --dry-run   # affiche config.toml et les changements du .zshrc, n'écrit rien
zsh-tool migrate             # écrit config.toml et modifie le .zshrc (après une sauvegarde)
exec zsh
```

`migrate` lit les plugins et le thème du `.zshrc`, écrit `~/.config/zsh-tool/config.toml`, puis remplace `source $ZSH/oh-my-zsh.sh` par `eval "$(zsh-tool init zsh)"`. Le `.zshrc` d'origine est copié dans `.zshrc.zsh-tool.bak`. Les plugins et thèmes restent ceux du dossier Oh My Zsh, qu'il faut donc garder. Le [guide de migration](docs/MIGRATION.md) détaille ce qui est lu, ce qui change, comment revenir en arrière et les limites connues.

### Configuration manuelle

Créer `~/.config/zsh-tool/config.toml` :

```toml
plugins = ["git", "sudo", "z"]
```

Puis, dans `~/.zshrc` :

```sh
eval "$(zsh-tool init zsh)"
```

Et lancer `zsh-tool doctor` pour vérifier l'installation.

## Configuration

`~/.config/zsh-tool/config.toml` (ou `$XDG_CONFIG_HOME/zsh-tool/config.toml`, ou `$ZSH_TOOL_CONFIG`) :

```toml
plugins = ["git", "sudo", "z"]

[prompt]
engine = "none"   # "none", "starship" ou "legacy"
theme = "robbyrussell"

[compat]
lib = false       # true : charge aussi les fichiers lib/ d'Oh My Zsh
```

Tous les champs sont facultatifs ; un fichier absent équivaut à une configuration vide. Un champ inconnu ou une erreur de syntaxe est signalé avec son numéro de ligne.

Les noms de plugins ne peuvent contenir que des lettres, des chiffres, `.`, `_` et `-`, ne commencent ni par `.` ni par `-` et ne contiennent pas `..` ; un nom invalide dans `config.toml` est une erreur, avec son numéro de ligne.

Les plugins sont cherchés dans cet ordre : `~/.local/share/zsh-tool/plugins`, `$ZSH_CUSTOM/plugins`, puis `$ZSH/plugins` (Oh My Zsh, par défaut `~/.oh-my-zsh`). Un plugin introuvable est signalé par un commentaire dans le script, sans bloquer le shell.

Avec `compat.lib = true`, les fichiers `$ZSH/lib/*.zsh` d'Oh My Zsh sont chargés avant les plugins, comme le fait Oh My Zsh. Cette option ralentit le démarrage.

### Cache

Le script est mis en cache dans `~/.cache/zsh-tool/init.zsh` et régénéré quand la configuration, un plugin, le thème, le binaire `starship`, un fichier `lib/` chargé ou la version de zsh-tool change : il n'y a jamais besoin de le vider à la main. `compinit` n'est lancé qu'une fois, après l'ajout de tous les `fpath`, avec un fichier de cache dans le même dossier, compilé par `zcompile` au premier démarrage pour être relu plus vite.

## Moteurs de prompt

- `none` (par défaut) : zsh-tool ne touche pas au prompt ;
- `starship` : le script lance `eval "$(starship init zsh)"` si `starship` est dans le `PATH`, sinon il contient seulement un commentaire ;
- `legacy` : charge le thème Oh My Zsh `<theme>.zsh-theme`, cherché dans `~/.local/share/zsh-tool/themes`, `$ZSH_CUSTOM/themes`, `$ZSH_CUSTOM` puis `$ZSH/themes`, après les plugins et `compinit`. zsh-tool fournit lui-même les fonctions utilisées par les thèmes (`git_prompt_info`, `parse_git_dirty`, `git_prompt_status`, couleurs...), sans dépendre de `lib/` d'Oh My Zsh. Un thème introuvable est signalé sur la sortie d'erreur, sans bloquer le shell.

La liste complète des fonctions fournies aux thèmes est dans le [guide de migration](docs/MIGRATION.md#thèmes-legacy-et-lib).

## Gérer les plugins

```sh
zsh-tool plugin add git                              # déjà installé (zsh-tool, ZSH_CUSTOM, Oh My Zsh) : activé seulement
zsh-tool plugin add zsh-users/zsh-autosuggestions    # dépôt GitHub, cloné en HTTPS
zsh-tool plugin add https://example.com/me/plug.git --name plug
zsh-tool plugin list                                 # nom, provenance, source et commit
zsh-tool plugin update [nom…]                        # tous les plugins gérés par défaut
zsh-tool plugin remove plug
```

`plugin add` accepte :

- un nom : si le plugin est déjà présent dans l'un des dossiers de recherche, il est seulement ajouté à `config.toml` ; sinon il est copié depuis le registre officiel (`plugins/<nom>/` du dépôt `https://github.com/maxg56/zsh-tool-plugins.git`, cloné dans `~/.local/share/zsh-tool/registry`) ;
- `owner/repo` (GitHub) ou une URL `https://` : le dépôt est cloné dans `~/.local/share/zsh-tool/plugins/<nom>`, où le nom est le dernier segment de l'URL sans `.git` (option `--name` pour le changer) ;
- un chemin absolu vers un dépôt Git local, pour tester un plugin en cours d'écriture.

Seul HTTPS est accepté pour les dépôts distants : `http://`, `git://`, `ssh://` et `git@…` sont refusés. Le plugin est ajouté à la fin de `plugins` dans `config.toml` (créé au besoin), en conservant commentaires et mise en forme ; relancer la commande ne change rien. Aucun code du plugin n'est exécuté avant que le shell ne le charge.

`plugin remove` retire le plugin de `config.toml` et ne supprime son dossier que s'il a été installé par zsh-tool : rien n'est jamais supprimé dans Oh My Zsh ou `ZSH_CUSTOM`. `plugin update` refuse de toucher un plugin dont le dossier a été modifié (fichier changé, ajouté ou supprimé).

Le client Git est intégré (gix, TLS via rustls) : la commande `git` n'est pas nécessaire pour les dépôts HTTPS (seuls les chemins locaux passent par `git-upload-pack`). Le proxy (`HTTPS_PROXY`) et le certificat d'autorité (`http.sslCAInfo`, `GIT_SSL_CAINFO`) de la configuration Git sont pris en compte.

## Verrouillage et mises à jour

Chaque plugin installé par zsh-tool est inscrit dans `zsh-tool.lock`, à côté de `config.toml`, avec sa source (URL ou `registry`) et le commit exact, triés par nom :

```toml
[[plugin]]
name = "zsh-autosuggestions"
source = "https://github.com/zsh-users/zsh-autosuggestions.git"
commit = "85919cd1ffa7d2d5412f6d3fe437ebdbeeec4fc5"
```

```sh
zsh-tool upgrade   # met à jour le registre et tous les plugins gérés, affiche ancien -> nouveau commit
zsh-tool install   # réinstalle les plugins manquants au commit du lock (par exemple sur une autre machine)
```

Pour retrouver la même configuration sur une autre machine, il suffit d'y copier `config.toml` et `zsh-tool.lock` puis de lancer `zsh-tool install`. Il n'est pas nécessaire de vider le cache après ces commandes : il est régénéré dès que `config.toml` ou un plugin change.

## Diagnostic

```sh
zsh-tool doctor
```

`doctor` vérifie, sans accès réseau : la configuration (avec la ligne d'une erreur), que chaque plugin est trouvé, que les plugins gérés sont présents et identiques au commit du lock, que zsh 5.8 ou plus est dans le `PATH`, que Starship est installé si `engine = "starship"`, la présence d'Oh My Zsh et l'accès en écriture au cache. Chaque ligne est marquée `ok`, `info`, `warning` ou `error` ; le code de sortie est non nul s'il y a au moins une erreur.

## Plugins natifs

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

Les variables de tous les plugins sont exportées avant le premier `source`, puis chaque plugin définit ses alias et source ses fragments, dans l'ordre de la configuration. Un manifeste invalide n'empêche pas le shell de démarrer : le plugin est ignoré, avec un commentaire dans le script et un message (fichier et ligne) sur la sortie d'erreur. Le format complet est décrit dans la [spécification de plugin.toml](docs/PLUGIN_TOML.md).

Des portages natifs des 20 plugins Oh My Zsh de référence (`git`, `sudo`, `z`, `extract`, `colored-man-pages`, `history`, `docker`, `docker-compose`, `kubectl`, `npm`, `node`, `nvm`, `python`, `pip`, `pyenv`, `fzf`, `command-not-found`, `web-search`, `copypath`, `dirhistory`) sont publiés dans le dépôt [maxg56/zsh-tool-plugins](https://github.com/maxg56/zsh-tool-plugins), le registre par défaut de `zsh-tool plugin add <nom>`. Ils ne dépendent pas de `lib/` d'Oh My Zsh. Les portages `nvm` et `pyenv` chargent leur outil au premier appel (`nvm`, `node`, `npm`… ou `pyenv`, `python`, `pip`…) ; `zstyle ':omz:plugins:nvm' lazy no` (ou `':omz:plugins:pyenv'`), placé avant `eval "$(zsh-tool init zsh)"`, rétablit le chargement au démarrage.

## Variables d'environnement

| Variable | Rôle |
| --- | --- |
| `ZSH_TOOL_CONFIG` | Chemin de `config.toml` (et donc de `zsh-tool.lock`) |
| `ZSH_TOOL_HOME` | Dossier des plugins, du registre et des thèmes de zsh-tool (par défaut `~/.local/share/zsh-tool`) |
| `ZSH_TOOL_REGISTRY` | Dépôt du registre (URL `https://` ou chemin local absolu) |
| `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME` | Remplacent `~/.config`, `~/.local/share` et `~/.cache` dans les chemins par défaut |
| `ZSH`, `ZSH_CUSTOM`, `ZSH_CACHE_DIR` | Emplacements Oh My Zsh, transmis aux plugins |

## Performance

Mesures du jalon J5 sur la machine de référence (conteneur Linux x86_64, 4 vCPU, zsh 5.9), `zsh -i -c exit` mesuré par hyperfine, 50 exécutions :

| Configuration | 10 plugins | 20 plugins |
| --- | ---: | ---: |
| Oh My Zsh | 63,0 ± 6,7 ms | 75,8 ± 9,5 ms |
| zsh-tool, plugins Oh My Zsh | 37,3 ± 4,1 ms | 44,4 ± 5,2 ms |
| zsh-tool, plugins natifs | 35,7 ± 4,7 ms | 44,4 ± 4,3 ms |
| `zsh-tool init zsh` seul (cache valide) | 3,8 ± 0,5 ms | 4,0 ± 1,0 ms |

Sur les 20 plugins de référence, 19 se chargent sans erreur, avec zsh-tool comme avec Oh My Zsh ; le vingtième, `fzf`, affiche volontairement une erreur quand fzf n'est pas installé. La méthode, la machine, la liste des plugins et la façon de refaire les mesures sont dans [docs/MESURES.md](docs/MESURES.md).

## Développement

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                       # les tests d'intégration utilisent zsh s'il est installé, et git pour préparer des dépôts locaux
ZSH_TOOL_REQUIRE_ZSH=1 cargo test  # échoue si zsh est absent (CI)
```

Les tests de migration et de thèmes utilisent un extrait d'Oh My Zsh copié dans `tests/fixtures/ohmyzsh` (licence MIT) et demandent `git`. La CI (`.github/workflows/ci.yml`) lance ces commandes sous Linux et macOS, et vérifie la compilation avec la version minimale de Rust. La procédure de publication d'une version est décrite dans [docs/RELEASE.md](docs/RELEASE.md).

Mesures de démarrage ([docs/MESURES.md](docs/MESURES.md)) :

```sh
scripts/bench.sh --native ../zsh-tool-plugins/plugins /tmp/omz           # temps, 10 plugins
scripts/bench.sh --native ../zsh-tool-plugins/plugins --set 20 /tmp/omz  # temps, 20 plugins
scripts/bench.sh --check --native ../zsh-tool-plugins/plugins /tmp/omz   # chargement sans erreur, X/20
```

## Documentation

- [Guide de migration depuis Oh My Zsh](docs/MIGRATION.md)
- [Spécification du format plugin.toml](docs/PLUGIN_TOML.md)
- [Mesures de démarrage](docs/MESURES.md)
- [Publier une version](docs/RELEASE.md)
- [Cahier des charges](docs/CAHIER_DES_CHARGES.md)

## Licence

MIT, voir [LICENSE](LICENSE).
