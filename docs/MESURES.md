# Mesures de démarrage

Rapport du jalon J5 : temps de démarrage sous Oh My Zsh et sous zsh-tool (plugins Oh My Zsh chargés en mode compatibilité, puis plugins natifs `plugin.toml`), et chargement sans erreur des 20 plugins de référence. Toutes les mesures viennent de `scripts/bench.sh` ; les mesures du jalon J1 sont rappelées à la fin.

## Résultats

| Objectif | Cible | Résultat | |
| --- | --- | --- | --- |
| O1 : démarrage avec 10 plugins | ≤ 100 ms | 35,7 ± 4,7 ms (natif), 37,3 ± 4,1 ms (plugins Oh My Zsh) ; Oh My Zsh : 63,0 ± 6,7 ms | Atteint |
| O2 : plugins de référence chargés sans erreur | ≥ 18/20 | 19/20 (natif quand il existe), 19/20 (plugins Oh My Zsh seuls) ; Oh My Zsh : 19/20 | Atteint |

Le seul plugin en échec en natif est `fzf`, qui écrit volontairement un message d'erreur quand fzf n'est pas installé, exactement comme sous Oh My Zsh (voir [Limites](#limites-connues)).

## Les 20 plugins de référence

Oh My Zsh ne publie pas de statistiques d'usage de ses 372 plugins (commit `d2b7f55`) et cette machine n'a pas d'accès à des sondages : la liste n'est donc pas un classement mesuré, mais une sélection argumentée, à revoir si des chiffres publics deviennent disponibles. Critères : présence dans le modèle de `.zshrc` d'Oh My Zsh, outils de développement les plus répandus (Git, conteneurs, Node.js, Python), plugins sans dépendance cités dans la plupart des guides de configuration, et présence obligatoire dans le dépôt Oh My Zsh (les plugins externes comme zsh-autosuggestions et zsh-syntax-highlighting sont exclus).

| # | Plugin | Pourquoi |
| --- | --- | --- |
| 1 | `git` | Seul plugin activé par le modèle `templates/zshrc.zsh-template` : présent chez presque tous les utilisateurs |
| 2 | `sudo` | Sans dépendance, très souvent ajouté au modèle |
| 3 | `z` | Navigation rapide entre dossiers, sans dépendance |
| 4 | `extract` | Sans dépendance, usage courant |
| 5 | `colored-man-pages` | Sans dépendance, usage courant |
| 6 | `history` | Alias d'historique, sans dépendance |
| 7 | `docker` | Outil de conteneurs le plus répandu |
| 8 | `kubectl` | Client Kubernetes |
| 9 | `npm` | Gestionnaire de paquets Node.js |
| 10 | `python` | Langage parmi les plus utilisés |
| 11 | `docker-compose` | Compagnon de `docker` |
| 12 | `node` | Node.js |
| 13 | `nvm` | Gestionnaire de versions Node.js ; plugin lourd (cité par le cahier des charges) |
| 14 | `pip` | Gestionnaire de paquets Python |
| 15 | `pyenv` | Gestionnaire de versions Python ; plugin lourd (cité par le cahier des charges) |
| 16 | `fzf` | Recherche floue, très répandue |
| 17 | `command-not-found` | Suggestion de paquet de la distribution |
| 18 | `web-search` | Sans dépendance, usage courant |
| 19 | `copypath` | Sans dépendance, usage courant |
| 20 | `dirhistory` | Sans dépendance, usage courant |

Les 10 premiers forment la configuration de l'objectif O1 (la même qu'au jalon J1). Les 20 ont un portage natif dans le dépôt `maxg56/zsh-tool-plugins` (`plugins/<nom>/plugin.toml`).

## Machine de référence

- Conteneur Linux x86_64 (noyau 6.18.44), Ubuntu 24.04.5, 4 vCPU Intel Xeon @ 2,80 GHz (1 thread par cœur), 15 Go de mémoire ; machine partagée, d'où des écarts-types de 4 à 10 ms.
- zsh 5.9, hyperfine 2.0.0, Oh My Zsh au commit `d2b7f55`, aucun thème, zsh-tool au commit de ce rapport (compilé en release par le script), plugins natifs du dépôt `zsh-tool-plugins` (20 plugins).
- Outils présents : git, docker 29.9.2 (client), node 22 et npm, python 3 et pip, ripgrep. Absents : kubectl, docker-compose (v1), nvm, pyenv, fzf, gestionnaire `command-not-found`.

## Méthode

`scripts/bench.sh` crée un dossier personnel isolé par configuration, avec le même `PATH` que l'appelant et rien d'autre dans l'environnement (`env -i`), et mesure `zsh -i -c exit` avec hyperfine (3 exécutions de chauffe pour remplir les caches, puis 50 mesures) :

- **oh-my-zsh** : `.zshrc` standard (`plugins=(…)`, `source $ZSH/oh-my-zsh.sh`) ;
- **zsh-tool legacy** : `eval "$(zsh-tool init zsh)"`, les plugins sont les `*.plugin.zsh` d'Oh My Zsh ;
- **zsh-tool natif** : idem, avec le dossier `plugins` de zsh-tool-plugins dans `$ZSH_TOOL_HOME/plugins` (prioritaire sur Oh My Zsh) ;
- **`zsh-tool init zsh` (cache valide)** : le binaire seul.

Le mode `--check` démarre `zsh -i -c exit` deux fois (à froid, puis avec les caches) pour chaque plugin seul, sous les trois configurations ; un démarrage échoue s'il écrit quoi que ce soit sur la sortie d'erreur ou si son code de sortie n'est pas 0.

### Refaire les mesures

```sh
git clone https://github.com/ohmyzsh/ohmyzsh.git /tmp/omz && git -C /tmp/omz checkout d2b7f55
git clone https://github.com/maxg56/zsh-tool-plugins.git /tmp/ztp
N=/tmp/ztp/plugins
RUNS=50 scripts/bench.sh --native $N /tmp/omz             # 10 plugins (O1)
RUNS=50 scripts/bench.sh --native $N --set 20 /tmp/omz    # 20 plugins
scripts/bench.sh --check --native $N /tmp/omz             # O2 : décompte X/20
scripts/bench.sh /tmp/omz git sudo z                      # liste personnalisée
```

Variables utiles : `RUNS`, `EXPORT` (fichier Markdown de résultats), `PRELUDE` (code zsh placé en tête de chaque `.zshrc`, par exemple des `zstyle`), `ZSH_COMMAND` (commande mesurée à la place de `exit`) ; `PATH`, `NVM_DIR` et `PYENV_ROOT` sont transmis aux shells mesurés. Sur une autre machine, noter comme ci-dessus le processeur, le nombre de cœurs, la version de zsh et le commit d'Oh My Zsh, et les outils installés : un plugin dont l'outil est absent ne fait presque rien, ce qui change les temps.

Pour profiler un shell fonction par fonction, ajouter `zmodload zsh/zprof` en tête du `.zshrc` et `zprof` à la fin, puis lancer `zsh -i -c exit`.

## Temps de démarrage

### 10 plugins

git, sudo, z, extract, colored-man-pages, history, docker, kubectl, npm, python.

| Commande | Moyenne (ms) | Écart avec Oh My Zsh |
|:---|---:|---:|
| Oh My Zsh | 63,0 ± 6,7 | |
| zsh-tool legacy | 37,3 ± 4,1 | −41 % |
| zsh-tool natif | 35,7 ± 4,7 | −43 % |
| `zsh-tool init zsh` (cache valide) | 3,8 ± 0,5 | |

### 20 plugins

Les 10 précédents plus docker-compose, node, nvm, pip, pyenv, fzf, command-not-found, web-search, copypath, dirhistory (nvm, pyenv et fzf ne trouvent pas leur outil et ne font presque rien).

| Commande | Moyenne (ms) | Écart avec Oh My Zsh |
|:---|---:|---:|
| Oh My Zsh | 75,8 ± 9,5 | |
| zsh-tool legacy | 44,4 ± 5,2 | −41 % |
| zsh-tool natif | 44,4 ± 4,3 | −41 % |
| `zsh-tool init zsh` (cache valide) | 4,0 ± 1,0 | |

### Goulot corrigé : `compinit`

Avant la correction, la même mesure donnait (même jour, même machine) :

| Commande | 10 plugins (ms) | 20 plugins (ms) |
|:---|---:|---:|
| Oh My Zsh | 59,4 ± 8,2 | 71,8 ± 7,8 |
| zsh-tool legacy | 42,9 ± 4,0 | 51,0 ± 5,0 |
| zsh-tool natif | 43,0 ± 4,4 | 52,5 ± 4,5 |

Le profil `zprof` du shell natif à 10 plugins attribuait 14,9 ms à `compinit` seul, soit 73 % du temps mesuré dans des fonctions zsh ; venait ensuite l'initialisation du plugin git (4,3 ms). `compinit -C` relit à chaque démarrage le fichier `zcompdump` (50 ko de script zsh). zsh-tool compile désormais ce fichier avec `zcompile` une fois après sa création (dans un fichier temporaire renommé ensuite, pour qu'un shell lancé en parallèle ne lise jamais un fichier partiel) ; `source` charge alors automatiquement la version compilée. Gain mesuré : 7 à 8 ms par démarrage (43,0 → 35,7 ms à 10 plugins). Le fichier compilé est supprimé avec le `zcompdump` à chaque régénération du script.

### Effet du chargement paresseux (nvm, pyenv)

Les plugins natifs `nvm` et `pyenv` chargent leur outil au premier appel (fonctions-relais `nvm`, `node`, `npm`, `npx`… et `pyenv`, `python`, `python3`, `pip`, `pip3`), par défaut ; `zstyle ':omz:plugins:nvm' lazy no` et `zstyle ':omz:plugins:pyenv' lazy no` rétablissent le chargement au démarrage. Sous Oh My Zsh, nvm n'est paresseux qu'avec `zstyle ':omz:plugins:nvm' lazy yes` et pyenv ne l'est jamais.

nvm et pyenv ne sont pas installés sur la machine de référence et l'exécution de leurs scripts téléchargés n'a pas été autorisée dans cet environnement. Les mesures ci-dessous utilisent donc des **substituts minimaux** : un `nvm.sh` qui définit la fonction `nvm` et ajoute un dossier à `PATH`, et un script `pyenv` qui répond aux appels du plugin (`init`, `commands`…). Le vrai nvm.sh (plusieurs milliers de lignes, résolution de la version par défaut) et le vrai `pyenv init` (plusieurs processus bash) coûtent beaucoup plus : l'écart mesuré ici est une **borne basse** du gain. Configuration : les 10 plugins plus nvm et pyenv, `PATH` réduit aux dossiers système et à ceux des substituts (npm n'y est pas).

| Commande (12 plugins, `zsh -i -c exit`) | Moyenne (ms) |
|:---|---:|
| Oh My Zsh (nvm et pyenv chargés au démarrage) | 72,4 ± 7,9 |
| Oh My Zsh, `nvm lazy yes` | 70,0 ± 9,4 |
| zsh-tool legacy (nvm et pyenv chargés au démarrage) | 40,3 ± 3,8 |
| zsh-tool natif, paresseux (défaut) | 33,4 ± 4,1 |
| zsh-tool natif, `lazy no` pour nvm et pyenv | 37,9 ± 3,8 |

| Commande (12 plugins, `zsh -i -c 'node --version'`) | Moyenne (ms) |
|:---|---:|
| zsh-tool natif, paresseux : nvm chargé au premier appel de `node` | 38,2 ± 4,3 |
| zsh-tool natif, `lazy no` | 42,4 ± 4,7 |

Au démarrage, la version paresseuse n'exécute ni nvm.sh ni pyenv : son coût ne dépend plus de ces outils (33,4 ms, du même ordre que les 10 plugins seuls). Avec les substituts, le chargement immédiat coûte 4,5 ms de plus (deux processus pour pyenv) ; avec les vrais outils l'écart est plus grand, mais il n'a pas pu être mesuré ici. Le coût est reporté sur le premier appel de `node` ou `python`, une fois par shell. Un contrôle de bon fonctionnement (sans mesure de temps) a aussi été fait avec les vrais nvm 0.40.3 et pyenv 2.6.11 : au premier appel, les fonctions-relais chargent l'outil puis exécutent `node --version`, `pyenv --version` et `python3 --version`, sans message d'erreur.

Pour mesurer avec les vrais outils sur une autre machine :

```sh
export NVM_DIR=~/.nvm PYENV_ROOT=~/.pyenv PATH=~/.pyenv/shims:~/.pyenv/bin:$PATH
P="git sudo z extract colored-man-pages history docker kubectl npm python nvm pyenv"
RUNS=50 scripts/bench.sh --native $N /tmp/omz $P
RUNS=50 PRELUDE="zstyle ':omz:plugins:nvm' lazy no; zstyle ':omz:plugins:pyenv' lazy no" \
    scripts/bench.sh --native $N /tmp/omz $P
RUNS=50 ZSH_COMMAND='node --version' scripts/bench.sh --native $N /tmp/omz $P
```

## Chargement des 20 plugins

`scripts/bench.sh --check --native …` sur la machine de référence :

| Plugin | Oh My Zsh | zsh-tool legacy | zsh-tool natif | Remarque |
|:---|:---:|:---:|:---:|:---|
| git | ok | ok | ok | |
| sudo | ok | ok | ok | |
| z | ok | ok | ok | |
| extract | ok | ok | ok | |
| colored-man-pages | ok | ok | ok | |
| history | ok | ok | ok | |
| docker | ok | ok | ok | voir ci-dessous |
| kubectl | ok | ok | ok | no-op : kubectl absent |
| npm | ok | ok | ok | |
| python | ok | ok | ok | |
| docker-compose | ok | ok | ok | alias vers `docker compose` |
| node | ok | ok | ok | |
| nvm | ok | ok | ok | no-op : nvm absent |
| pip | ok | ok | ok | |
| pyenv | ok | ok | ok | pyenv absent : seule `pyenv_prompt_info` est définie |
| fzf | **échec** | **échec** | **échec** | fzf absent : message d'erreur voulu |
| command-not-found | ok | ok | ok | no-op : aucun gestionnaire installé |
| web-search | ok | ok | ok | |
| copypath | ok | ok | ok | |
| dirhistory | ok | ok | ok | |
| **Total** | **19/20** | **19/20** | **19/20** | |

En complément, avec des substituts de nvm, pyenv et fzf dans `PATH` (`NVM_DIR`, `PYENV_ROOT` et un `fzf` factice qui répond à `--version` et `--zsh`), `nvm`, `pyenv` et `fzf` se chargent sans erreur dans les trois configurations. Les vérifications fonctionnelles de chaque plugin natif (alias, fonctions, complétions, widgets, chargement paresseux) sont dans `scripts/check.sh` du dépôt de plugins.

## Limites connues

- **fzf** : sans fzf installé, le plugin écrit « Cannot find fzf installation directory » sur la sortie d'erreur, sous Oh My Zsh comme en natif (préfixe `[zsh-tool]` au lieu de `[oh-my-zsh]`). C'est le comportement voulu du plugin ; il se charge sans erreur dès que fzf est installé.
- **docker en mode legacy** : le `docker.plugin.zsh` d'Oh My Zsh appelle `is-at-least` sans le charger (`autoload`), car c'est `lib/` d'Oh My Zsh qui le fait. La première vérification affichait donc `command not found: is-at-least` en mode legacy (18/20). Le script généré fait désormais `autoload -Uz is-at-least` avec les variables de compatibilité, sans coût mesurable puisque la fonction n'est chargée qu'au premier appel : docker passe en legacy (19/20). Le plugin natif charge lui-même `is-at-least`.
- **Fonctions de `lib/` d'Oh My Zsh** : en mode legacy, `node-docs`, `web_search` et `copypath` se chargent mais échouent à l'usage (`open_command`, `omz_urlencode`, `clipcopy` absents) ; les plugins natifs embarquent leur propre copie de ces fonctions.
- **Chargement paresseux** : avant le premier appel d'une fonction-relais, la version de Node.js choisie par nvm n'est pas dans `PATH` (un script `#!/usr/bin/env node` lancé depuis le shell utilise le `node` du système, s'il y en a un) et le hook de pyenv-virtualenv (activation automatique) n'est pas installé ; les shims de pyenv, eux, sont dans `PATH` dès le démarrage. `lazy no` rétablit le comportement d'Oh My Zsh.
- **Complétions générées** (docker, kubectl) : Oh My Zsh lance `compinit` avant ses plugins, zsh-tool après ; les plugins natifs enregistrent donc toujours leur complétion par `autoload` + `compdef` au lieu de compter sur le `zcompdump`.
- **Environnement de mesure** : conteneur partagé, écarts-types de 4 à 10 ms ; les comparaisons portent sur des mesures faites l'une après l'autre le même jour.

## Mesures du jalon J1 (26 octobre 2026)

Même machine, avant les plugins natifs et la compilation du `zcompdump`, sans `PATH` transmis aux shells mesurés (zsh utilisait son `PATH` par défaut) : Oh My Zsh 65,4 ± 10,1 ms, zsh-tool 46,8 ± 5,7 ms (10 plugins), `zsh-tool init zsh` avec cache valide 4,1 ± 0,5 ms.
