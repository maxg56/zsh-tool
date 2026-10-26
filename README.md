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

Les variables de tous les plugins sont exportées avant le premier `source`, puis chaque plugin définit ses alias et source ses fragments, dans l'ordre de la configuration. Un manifeste invalide n'empêche pas le shell de démarrer : le plugin est ignoré, avec un commentaire dans le script et un message (fichier et ligne) sur la sortie d'erreur. Le format complet est décrit dans [docs/PLUGIN_TOML.md](docs/PLUGIN_TOML.md) ; des portages natifs des 20 plugins Oh My Zsh de référence (`git`, `sudo`, `z`, `extract`, `colored-man-pages`, `history`, `docker`, `docker-compose`, `kubectl`, `npm`, `node`, `nvm`, `python`, `pip`, `pyenv`, `fzf`, `command-not-found`, `web-search`, `copypath`, `dirhistory`) sont publiés dans le dépôt [maxg56/zsh-tool-plugins](https://github.com/maxg56/zsh-tool-plugins), le registre par défaut de `zsh-tool plugin add <nom>`. Les portages `nvm` et `pyenv` chargent leur outil au premier appel (`nvm`, `node`, `npm`… ou `pyenv`, `python`, `pip`…) ; `zstyle ':omz:plugins:nvm' lazy no` (ou `':omz:plugins:pyenv'`), placé avant `eval "$(zsh-tool init zsh)"`, rétablit le chargement au démarrage.

Le script est mis en cache dans `~/.cache/zsh-tool/init.zsh` et régénéré quand la configuration, un plugin, le thème, le binaire `starship`, un fichier `lib/` chargé ou la version de zsh-tool change. `compinit` n'est lancé qu'une fois, après l'ajout de tous les `fpath`, avec un fichier de cache dans le même dossier, compilé par `zcompile` au premier démarrage pour être relu plus vite.

Les noms de plugins ne peuvent contenir que des lettres, des chiffres, `.`, `_` et `-`, ne commencent ni par `.` ni par `-` et ne contiennent pas `..` ; un nom invalide dans `config.toml` est une erreur, avec son numéro de ligne.

### Gérer les plugins

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

### Verrouillage et mises à jour

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

Il n'est pas nécessaire de vider le cache après ces commandes : il est régénéré dès que `config.toml` ou un plugin change.

### Diagnostic

```sh
zsh-tool doctor
```

`doctor` vérifie, sans accès réseau : la configuration (avec la ligne d'une erreur), que chaque plugin est trouvé, que les plugins gérés sont présents et identiques au commit du lock, que zsh 5.8 ou plus est dans le `PATH`, que Starship est installé si `engine = "starship"`, la présence d'Oh My Zsh et l'accès en écriture au cache. Chaque ligne est marquée `ok`, `info`, `warning` ou `error` ; le code de sortie est non nul s'il y a au moins une erreur.

| Variable | Rôle |
| --- | --- |
| `ZSH_TOOL_CONFIG` | Chemin de `config.toml` (et donc de `zsh-tool.lock`) |
| `ZSH_TOOL_HOME` | Dossier des plugins, du registre et des thèmes de zsh-tool |
| `ZSH_TOOL_REGISTRY` | Dépôt du registre (URL `https://` ou chemin local absolu) |
| `ZSH`, `ZSH_CUSTOM`, `ZSH_CACHE_DIR` | Emplacements Oh My Zsh, transmis aux plugins |

Le client Git est intégré (gix, TLS via rustls) : la commande `git` n'est pas nécessaire pour les dépôts HTTPS (seuls les chemins locaux passent par `git-upload-pack`). Le proxy (`HTTPS_PROXY`) et le certificat d'autorité (`http.sslCAInfo`, `GIT_SSL_CAINFO`) de la configuration Git sont pris en compte.

## Développement

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                       # les tests d'intégration utilisent zsh s'il est installé, et git pour préparer des dépôts locaux
ZSH_TOOL_REQUIRE_ZSH=1 cargo test  # échoue si zsh est absent (CI)
```

Les tests de migration et de thèmes utilisent un extrait d'Oh My Zsh copié dans `tests/fixtures/ohmyzsh` (licence MIT) et demandent `git`.

Les mesures de démarrage et la façon de les reproduire sont dans [docs/MESURES.md](docs/MESURES.md) :

```sh
scripts/bench.sh --native ../zsh-tool-plugins/plugins /tmp/omz           # temps, 10 plugins
scripts/bench.sh --native ../zsh-tool-plugins/plugins --set 20 /tmp/omz  # temps, 20 plugins
scripts/bench.sh --check --native ../zsh-tool-plugins/plugins /tmp/omz   # chargement sans erreur, X/20
```
