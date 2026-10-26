# Format de plugin natif : `plugin.toml`

Un plugin zsh-tool est un dossier. S'il contient un fichier `plugin.toml`, c'est un plugin **natif** ; sinon, s'il contient `<nom>.plugin.zsh` (ou seulement une fonction de complétion `_<nom>`), il est chargé en mode compatibilité Oh My Zsh. Quand les deux existent dans le même dossier, `plugin.toml` est prioritaire et le fichier `*.plugin.zsh` est ignoré.

Ce document décrit le format natif : ses champs, les règles de validation, ce que zsh-tool génère et dans quel ordre.

## Exemple

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
description = "Alias git courants"
init = ["init.zsh"]          # fragments zsh sourcés dans l'ordre
fpath = ["completions"]      # dossiers ajoutés à fpath

[aliases]
gst = "git status"
glg = "git log --oneline --graph"
"gc!" = "git commit --verbose --amend"

[env]
GIT_EXTRAS_DEBUG = "0"
```

## Champs

| Champ | Type | Obligatoire | Effet dans le script généré |
| --- | --- | --- | --- |
| `plugin.name` | chaîne | oui | Doit être égal au nom du dossier ; affiché en commentaire |
| `plugin.version` | chaîne | non | Métadonnée (commentaire, `zsh-tool plugin list`) |
| `plugin.description` | chaîne | non | Métadonnée (commentaire, `zsh-tool plugin list`) |
| `plugin.init` | liste de chemins | non (vide) | `source` de chaque fichier, dans l'ordre de la liste |
| `plugin.fpath` | liste de dossiers | non (vide) | Ajout en tête de `fpath`, avant `compinit` |
| `[aliases]` | table de chaînes | non | Une ligne `alias -- nom='valeur'` par entrée |
| `[env]` | table de chaînes | non | Une ligne `export NOM='valeur'` par entrée |

Aucun autre champ ni aucune autre table n'est accepté : une faute de frappe (`descripton`, `[alias]`) est une erreur plutôt qu'un réglage silencieusement ignoré.

## Règles de validation

- **Syntaxe** : TOML. Toute erreur indique le fichier et la ligne, par exemple `…/plugins/demo/plugin.toml, line 3: unknown field `homepage``.
- **`name`** : non vide, composé uniquement de lettres ASCII, chiffres, `.`, `_` et `-`, et égal au nom du dossier du plugin (`plugin name `foo` does not match its directory `bar``).
- **`init` et `fpath`** : chemins relatifs au dossier du plugin, non vides, sans composant `..` et non absolus. Chaque entrée de `init` doit désigner un fichier existant, chaque entrée de `fpath` un dossier existant.
- **Noms d'alias** : non vides, sans espace ni caractère de contrôle, sans `=`, `'` ni `"`. Les autres caractères sont permis (`gca!`, `...`) ; un nom qui contient des caractères spéciaux pour zsh est mis entre apostrophes dans le script.
- **Noms de variables** : `[A-Za-z_][A-Za-z0-9_]*`.
- **Valeurs** d'alias et de variables : chaînes uniquement (`DEBUG = 0` est refusé, écrire `DEBUG = "0"`).

Toutes les valeurs écrites dans le script sont échappées entre apostrophes : `$`, `` ` ``, `;` ou les guillemets n'y sont jamais interprétés au chargement. Une valeur d'alias est interprétée par zsh au moment où l'alias est utilisé, comme pour un `alias` écrit à la main.

### Manifeste invalide

Un manifeste invalide ne fait pas échouer `zsh-tool init zsh` : le plugin est ignoré (ni variable, ni alias, ni `source`), le script contient un commentaire sur une seule ligne

```zsh
# plugin demo: invalid manifest: /…/plugins/demo/plugin.toml, line 3: init file `init.zsh` does not exist
```

et le même message est affiché sur la sortie d'erreur à chaque démarrage du shell, tant que le manifeste n'est pas corrigé. La recherche ne passe pas au dossier suivant : un dossier qui contient `plugin.toml` est toujours celui du plugin, même invalide.

## Résolution

Un nom de plugin est cherché dans `~/.local/share/zsh-tool/plugins` (ou `$ZSH_TOOL_HOME/plugins`), puis `$ZSH_CUSTOM/plugins`, puis `$ZSH/plugins`. Le premier dossier qui contient un plugin (natif ou Oh My Zsh) est retenu.

## Ordre de génération

Pour une configuration `plugins = ["a", "b", "c"]`, le script `init.zsh` contient, dans cet ordre :

1. Les variables de compatibilité Oh My Zsh (`ZSH`, `ZSH_CUSTOM`, `ZSH_CACHE_DIR`).
2. Les `[env]` de **tous** les plugins natifs, plugin par plugin dans l'ordre de la configuration : un fragment peut donc lire une variable déclarée par un plugin listé après lui.
3. `fpath` : les dossiers `fpath` des plugins natifs et les dossiers des plugins Oh My Zsh, dans l'ordre de la configuration.
4. Pour chaque plugin, dans l'ordre de la configuration : ses alias déclaratifs, puis le `source` de chacun de ses fichiers `init`. Un fragment peut donc redéfinir un alias déclaré par son propre manifeste.
5. `compinit`, une seule fois ; les appels à `compdef` faits par les fragments avant `compinit` sont rejoués ensuite.

À l'intérieur de `[aliases]` et `[env]`, l'ordre de déclaration du fichier est conservé, ce qui rend le script déterministe.

Exemple de sortie pour le manifeste ci-dessus :

```zsh
# environment
export GIT_EXTRAS_DEBUG='0'

# fpath
typeset -gU fpath
fpath=('/…/plugins/git-extras/completions' "$ZSH_CACHE_DIR/completions" $fpath)

# plugins
# plugin git-extras 0.1.0: Alias git courants
alias -- gst='git status'
alias -- glg='git log --oneline --graph'
alias -- 'gc!'='git commit --verbose --amend'
source '/…/plugins/git-extras/init.zsh'
```

## Cache

Le script est régénéré quand la date de modification de `plugin.toml`, du dossier du plugin, d'un fichier `init` ou d'un dossier `fpath` change, ou quand le résultat de la validation change (par exemple quand un fichier `init` manquant est créé).

## Écrire les fragments `init`

- Les alias simples vont dans `[aliases]` ; les fonctions, widgets ZLE, raccourcis clavier et alias conditionnels (qui dépendent d'une version d'outil, par exemple) restent dans des fragments zsh.
- Les fragments sont sourcés avec `source` : `${(%):-%N}` ou `$0` y désignent le fichier sourcé, ce qui permet de retrouver le dossier du plugin.
- `compdef` peut être appelé avant `compinit` : l'appel est enregistré puis rejoué.
- Les fonctions de complétion `_nom` se placent dans un dossier déclaré dans `fpath` (par convention `completions/`).

## Différences avec les plugins Oh My Zsh

| | Plugin Oh My Zsh (`*.plugin.zsh`) | Plugin natif (`plugin.toml`) |
| --- | --- | --- |
| Détection | `<nom>.plugin.zsh` ou `_<nom>` | `plugin.toml` (prioritaire) |
| Métadonnées | aucune | `name`, `version`, `description` |
| Alias | lignes `alias` dans le script | table `[aliases]`, valeurs échappées par zsh-tool |
| Variables | lignes `export` dans le script | table `[env]`, définies avant tout plugin |
| `fpath` | le dossier du plugin est ajouté entier | seulement les dossiers déclarés |
| Code | un seul fichier sourcé | liste ordonnée de fragments `init` (éventuellement vide) |
| Dépendances | suppose souvent `lib/` d'Oh My Zsh (`git_current_branch`, `clipcopy`, couleurs…) | autonome : définit lui-même ses fonctions utilitaires |
| Erreurs | une erreur de syntaxe zsh apparaît au démarrage | le manifeste est validé par zsh-tool (fichier et ligne), le plugin est ignoré s'il est invalide |
