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

Les mesures de démarrage et la façon de les reproduire sont dans [docs/MESURES.md](docs/MESURES.md).
