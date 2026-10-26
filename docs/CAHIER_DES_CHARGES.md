# Cahier des charges — zsh-tool, framework zsh en Rust

Version du 26 octobre 2026.

## 1. Contexte et objectifs

zsh-tool remplace Oh My Zsh par un outil écrit en Rust qui génère, met en cache et charge la configuration zsh, pour démarrer vite tout en restant compatible avec les plugins existants.

**Problème.** Oh My Zsh source des dizaines de fichiers et lance `compinit` à chaque ouverture de shell. Avec 5 à 10 plugins, le démarrage se situe souvent entre 200 et 500 ms (estimation à confirmer par mesure sur la configuration cible).

**Objectifs (cibles proposées, à valider).**

- O1 : démarrage du shell ≤ 100 ms avec 10 plugins, mesuré avec `hyperfine 'zsh -i -c exit'`.
- O2 : au moins 90 % des 20 plugins Oh My Zsh les plus utilisés fonctionnent sans modification.
- O3 : un nouveau format de plugin déclaratif (`plugin.toml`), plus lisible et plus rapide à charger.
- O4 : migration automatique d'un `.zshrc` Oh My Zsh existant en une commande.
- O5 : prompt délégué à Starship, sans réécrire un moteur de prompt.
- O6 : projet open source publié (licence MIT), installable sans toolchain Rust.

## 2. Périmètre

zsh-tool couvre la gestion complète d'une configuration zsh (plugins, thèmes, migration, mises à jour) et s'appuie sur Starship pour le prompt.

**Inclus**

- Binaire `zsh-tool` : initialisation du shell, gestion des plugins, thèmes, migration, diagnostic, mise à jour.
- Format de plugin natif (`plugin.toml`) et couche de compatibilité avec les plugins `*.plugin.zsh` d'Oh My Zsh.
- Chargement des thèmes Oh My Zsh existants (`*.zsh-theme`) et intégration de Starship.
- Cache de l'init, chargement optimisé de `compinit`.
- Distribution : releases GitHub, `cargo install`, Homebrew.

**Exclus (version 1)**

- Moteur de prompt maison (Starship le fait déjà).
- Autres shells que zsh (bash et fish pourront venir après).
- Interface graphique.
- Portage natif des ~300 plugins : seuls les 10 à 20 plus utilisés sont réécrits au format natif, les autres restent chargés en mode compatibilité.

**Positionnement face à l'existant**

| Outil | Plugins | Compat. Oh My Zsh | Prompt | Migration depuis Oh My Zsh |
| --- | --- | --- | --- | --- |
| Oh My Zsh | Oui | Natif | Thèmes zsh | Sans objet |
| Starship | Non | Non | Oui (Rust) | Non |
| sheldon | Oui | Partielle (chargeur) | Non | Non |
| zsh-tool (cible) | Oui | Oui | Via Starship | Oui (automatique) |

## 3. Utilisateurs cibles et cas d'usage

La cible principale est l'utilisateur d'Oh My Zsh qui veut un démarrage plus rapide sans refaire sa configuration.

| Profil | Besoin | Cas d'usage |
| --- | --- | --- |
| Utilisateur d'Oh My Zsh | Garder ses plugins, gagner en vitesse | `zsh-tool migrate` puis `eval "$(zsh-tool init zsh)"` dans `.zshrc` |
| Développeur de plugins | Écrire un plugin simple et testable | Créer un `plugin.toml` avec alias déclaratifs et fragments zsh |
| Utilisateur avancé | Maîtriser l'ordre de chargement et la reproductibilité | Fichier de verrouillage `zsh-tool.lock`, `zsh-tool doctor` |
| Utilisateur de Starship | Un seul outil pour plugins et prompt | `engine = "starship"` dans la configuration |

## 4. Exigences fonctionnelles

Chaque exigence est numérotée (EF) pour servir de référence aux tests d'acceptation.

| Réf. | Exigence | Priorité |
| --- | --- | --- |
| EF1 | `zsh-tool init zsh` affiche sur la sortie standard un script zsh à évaluer par `eval` | Obligatoire |
| EF2 | Le script est mis en cache et régénéré uniquement si la configuration ou un plugin a changé (empreinte des fichiers et dates de modification) | Obligatoire |
| EF3 | Configuration dans `~/.config/zsh-tool/config.toml` : liste ordonnée de plugins, moteur de prompt, thème | Obligatoire |
| EF4 | Chargement de plugins natifs (`plugin.toml`) : alias, variables d'environnement, `fpath`, fragments zsh | Obligatoire |
| EF5 | Chargement de plugins Oh My Zsh (`*.plugin.zsh`) avec les variables `ZSH`, `ZSH_CUSTOM`, `ZSH_CACHE_DIR` définies | Obligatoire |
| EF6 | `zsh-tool plugin add <nom ou dépôt Git>` clone le plugin et l'active ; `remove`, `list`, `update` | Obligatoire |
| EF7 | `zsh-tool migrate` lit un `.zshrc` Oh My Zsh (`plugins=(...)`, `ZSH_THEME`) et produit la configuration zsh-tool équivalente | Obligatoire |
| EF8 | Moteur de prompt `starship` : le script appelle `starship init zsh` s'il est installé | Obligatoire |
| EF9 | Moteur `legacy` : chargement d'un thème `*.zsh-theme` d'Oh My Zsh | Importante |
| EF10 | `compinit` exécuté une seule fois, après l'ajout de tous les `fpath`, avec un fichier de cache | Obligatoire |
| EF11 | `zsh-tool doctor` vérifie l'installation : configuration, plugins introuvables, Starship, zsh, dossier Oh My Zsh | Importante |
| EF12 | `zsh-tool upgrade` met à jour les plugins ; verrouillage des versions dans `zsh-tool.lock` | Importante |
| EF13 | Un plugin introuvable ne bloque pas le shell : un commentaire est écrit dans le script et `doctor` le signale | Obligatoire |

## 5. Exigences non fonctionnelles

| Réf. | Domaine | Exigence |
| --- | --- | --- |
| ENF1 | Performance | `zsh-tool init zsh` avec cache valide s'exécute en moins de 10 ms ; démarrage complet du shell ≤ 100 ms avec 10 plugins (cibles à mesurer) |
| ENF2 | Performance | Régénération du cache en moins de 200 ms pour 20 plugins |
| ENF3 | Sécurité | Noms de plugins validés (caractères autorisés, pas de `..`) ; chaînes échappées pour zsh (guillemets simples) ; jamais d'exécution automatique de code distant sans action explicite de l'utilisateur |
| ENF4 | Sécurité | Dépôts de plugins clonés en HTTPS ; versions épinglées dans `zsh-tool.lock` |
| ENF5 | Portabilité | Linux et macOS (x86_64 et arm64) ; zsh 5.8 ou plus |
| ENF6 | Robustesse | Aucun `panic` sur une configuration invalide : message d'erreur clair avec numéro de ligne |
| ENF7 | Qualité | Tests unitaires et tests d'intégration exécutés avec un vrai `zsh -f` en CI ; `cargo clippy` sans avertissement |
| ENF8 | Distribution | Binaire statique unique, sans dépendance d'exécution autre que `git` pour les plugins distants |
| ENF9 | Documentation | README en français et en anglais, guide de migration, spécification du format de plugin |

## 6. Architecture technique

Un programme Rust ne peut pas modifier le shell qui l'appelle : zsh-tool génère donc un script zsh que le shell évalue au démarrage, comme `starship init zsh`.

**Flux d'initialisation**

1. `.zshrc` exécute `eval "$(zsh-tool init zsh)"`.
2. zsh-tool lit `config.toml` et calcule une empreinte (contenu de la configuration, dates de modification des plugins, version de zsh-tool).
3. Si l'empreinte correspond à celle du cache, zsh-tool affiche `~/.cache/zsh-tool/init.zsh` et s'arrête.
4. Sinon, zsh-tool résout chaque plugin (natif ou legacy), génère le script, écrit le cache et supprime l'ancien fichier de complétion.
5. Le shell exécute le script : variables d'environnement, alias, `fpath`, `source` des plugins, `compinit`, puis le prompt.

**Modules Rust**

| Module | Rôle |
| --- | --- |
| `main` | Analyse des arguments, dispatch des sous-commandes, codes de sortie |
| `config` | Lecture et écriture de `config.toml` (liste de plugins, moteur de prompt, thème) |
| `toml_lite` | Parseur TOML minimal (prototype) ; remplacé par la crate `toml` |
| `plugin` | Résolution des plugins dans l'ordre : données zsh-tool, `custom/plugins` d'Oh My Zsh, `plugins` d'Oh My Zsh ; lecture du manifeste |
| `init` | Génération du script zsh, empreinte, cache |
| `migrate` | Analyse d'un `.zshrc` Oh My Zsh et génération de la configuration zsh-tool |
| `doctor` | Diagnostic de l'installation |

**Emplacements**

- Configuration : `~/.config/zsh-tool/config.toml` (variable `ZSH_TOOL_CONFIG` pour changer).
- Plugins et thèmes : `~/.local/share/zsh-tool/` (variable `ZSH_TOOL_HOME`).
- Cache : `~/.cache/zsh-tool/`.

**Crates prévues** : `clap` (CLI), `serde` et `toml` (configuration), `gix` (Git sans processus externe), `anyhow` (erreurs). Le prototype actuel n'utilise que la bibliothèque standard, car le registre des crates n'est pas accessible depuis l'environnement de développement actuel.

## 7. Format de plugin et compatibilité Oh My Zsh

Un plugin est un dossier : s'il contient un `plugin.toml` il est natif, sinon s'il contient un fichier `*.plugin.zsh` il est chargé en mode compatibilité Oh My Zsh.

**Exemple de `plugin.toml` natif**

```toml
[plugin]
name = "git-extras"
version = "0.1.0"
description = "Alias git courants"
init = ["init.zsh"]          # fragments zsh sourcés dans l'ordre
fpath = ["completions"]      # dossiers ajoutés à fpath

[aliases]
gst = "git status"
glg = "git log --oneline --graph"

[env]
GIT_EXTRAS_DEBUG = "0"
```

**Champs du manifeste**

| Champ | Type | Effet dans le script généré |
| --- | --- | --- |
| `plugin.name`, `version`, `description` | texte | Métadonnées affichées par `zsh-tool plugin list` |
| `plugin.init` | liste de chemins | `source` de chaque fichier, dans l'ordre |
| `plugin.fpath` | liste de dossiers | Ajout en tête de `fpath` avant `compinit` |
| `[aliases]` | table | Une ligne `alias nom='valeur'` par entrée, valeurs échappées |
| `[env]` | table | Une ligne `export NOM='valeur'` par entrée |

**Règles de compatibilité Oh My Zsh**

- Variables définies avant tout plugin : `ZSH` (dossier Oh My Zsh s'il existe), `ZSH_CUSTOM`, `ZSH_CACHE_DIR`.
- Ordre de résolution d'un nom de plugin : dossier zsh-tool, `custom/plugins` d'Oh My Zsh, `plugins` d'Oh My Zsh.
- Le dossier d'un plugin legacy est ajouté à `fpath` (fichiers de complétion `_nom`).
- Les fichiers `lib/` d'Oh My Zsh ne sont pas chargés par défaut ; une option `compat.lib = true` est prévue pour les charger.
- Thèmes legacy : `setopt prompt_subst`, chargement des couleurs, puis `source` du fichier `*.zsh-theme`.

## 8. Planning par jalons

Le projet se déroule en six jalons successifs, pour environ 16 semaines à temps partiel (durées proposées, à ajuster selon votre disponibilité).

| Jalon | Contenu | Durée | Condition de passage |
| --- | --- | --- | --- |
| J1 — MVP | `init zsh`, configuration TOML, plugins legacy, cache, `compinit` | 3 semaines | EF1, EF2, EF3, EF5, EF10 validées ; démarrage mesuré |
| J2 — Gestion des plugins | `plugin add/remove/list/update`, `doctor`, `zsh-tool.lock` | 3 semaines | EF6, EF11, EF12, EF13 validées |
| J3 — Format natif | `plugin.toml`, alias déclaratifs, `fpath`, fragments | 3 semaines | EF4 validée ; 5 plugins portés au format natif |
| J4 — Migration et prompt | `zsh-tool migrate`, intégration Starship, thèmes legacy | 3 semaines | EF7, EF8, EF9 validées |
| J5 — Portage et performance | 10 à 20 plugins populaires en natif, mesures comparatives | 2 semaines | O1 et O2 atteints sur la machine de référence |
| J6 — Publication | Documentation, CI, releases GitHub, Homebrew, `cargo install` | 2 semaines | ENF7, ENF8, ENF9 validées ; version 0.1.0 publiée |

## 9. Risques et mitigations

Le risque principal est le périmètre : Oh My Zsh compte environ 300 plugins et 140 thèmes, et zsh-tool ne doit pas tenter de tous les réécrire.

| Risque | Probabilité | Impact | Mitigation |
| --- | --- | --- | --- |
| Périmètre trop large (300 plugins) | Élevée | Élevé | Compatibilité d'abord ; portage natif limité aux 10 à 20 plugins les plus utilisés |
| Bugs d'ordre de chargement et de `compinit` | Moyenne | Élevé | Tests d'intégration avec un vrai `zsh`, un seul appel à `compinit` après tous les `fpath` |
| Gain de performance inférieur à l'attendu | Moyenne | Moyen | Mesure dès J1 avec `hyperfine` et `zprof` ; chargement paresseux des plugins lourds (nvm, pyenv) |
| Plugins legacy dépendant de `lib/` d'Oh My Zsh | Moyenne | Moyen | Option `compat.lib` et liste des plugins incompatibles dans `doctor` |
| Exécution de code de dépôts tiers | Moyenne | Élevé | HTTPS uniquement, versions épinglées, aucune mise à jour silencieuse |
| Conflit de nom ou de marque | Faible | Moyen | Vérifier le nom sur crates.io et GitHub avant la publication |
| Disponibilité limitée (projet en parallèle d'autres projets) | Moyenne | Moyen | Jalons courts, MVP utilisable dès J1 |

## 10. Critères d'acceptation, livrables et questions ouvertes

La version 0.1.0 est acceptée quand les objectifs O1 à O6 sont atteints sur une machine de référence et que les exigences obligatoires sont validées.

**Critères d'acceptation**

- Un `.zshrc` Oh My Zsh avec 10 plugins, migré par `zsh-tool migrate`, donne un shell fonctionnel sans autre modification.
- Le démarrage est ≤ 100 ms avec 10 plugins, mesuré par `hyperfine` sur la machine de référence, contre la mesure de départ sous Oh My Zsh.
- Au moins 18 des 20 plugins Oh My Zsh les plus utilisés se chargent sans erreur.
- `zsh-tool doctor` ne signale aucune erreur sur une installation propre.
- La CI passe sur Linux et macOS avec un vrai `zsh`.

**Livrables**

- Code source Rust sur GitHub (licence MIT), tests et CI.
- Binaires de release Linux et macOS, formule Homebrew, publication sur crates.io.
- Documentation : README, guide de migration, spécification du format `plugin.toml`.
- Rapport de mesures comparatives (Oh My Zsh contre zsh-tool).

**Questions ouvertes**

- [ ] Nom définitif du projet (« zsh-tool » est provisoire) et disponibilité sur crates.io et GitHub.
- [ ] Liste de référence des 20 plugins à tester et machine de référence pour les mesures.
- [ ] Faut-il supporter bash et fish après la version 1 ?
- [ ] Compatibilité Windows (WSL uniquement ou natif) ?
- [ ] Langue de la documentation principale (français, anglais ou les deux dès la version 0.1.0).
