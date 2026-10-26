# Migrer depuis Oh My Zsh

[English](MIGRATION.en.md) | Français

`zsh-tool migrate` convertit un `.zshrc` Oh My Zsh : il écrit `config.toml` avec les mêmes plugins et le même thème, puis remplace le chargement d'Oh My Zsh par `zsh-tool init zsh`. Les plugins et thèmes restent ceux du dossier Oh My Zsh, qu'il faut donc garder.

## En bref

```sh
zsh-tool migrate --dry-run   # affiche config.toml et les changements du .zshrc, n'écrit rien
zsh-tool migrate             # fait la migration
exec zsh                     # nouveau shell
```

| Option | Effet |
| --- | --- |
| `--zshrc <chemin>` | Fichier à migrer (par défaut `$ZDOTDIR/.zshrc`, sinon `~/.zshrc`) |
| `--dry-run` | Affiche le `config.toml` prévu et un diff du `.zshrc`, sans rien écrire |
| `--force` | Écrase un `config.toml` existant (refusé sinon) |

`zsh-tool` doit être dans le `PATH` au moment où le `.zshrc` s'exécute (par exemple `~/.cargo/bin` ajouté dans `~/.zshenv`).

## Ce qui est lu

- `plugins=(...)`, sur une ou plusieurs lignes, avec commentaires ; les lignes commentées sont ignorées, `plugins+=(...)` ajoute à la liste, la dernière affectation l'emporte. Un nom qui ne peut pas être lu (`$var`, `$(...)`) ou qui contient d'autres caractères que lettres, chiffres, `.`, `_` et `-` est ignoré avec un avertissement.
- `ZSH_THEME`, entre guillemets doubles, simples ou sans guillemets.
- `ZSH` et `ZSH_CUSTOM` (avec ou sans `export`), où `~`, `$HOME` et `$ZSH` sont développés ; ils servent à vérifier que les plugins et le thème existent.
- Une ligne `eval "$(starship init zsh)"`.

## Moteur de prompt choisi

| `.zshrc` | `[prompt]` |
| --- | --- |
| Contient `eval "$(starship init zsh)"` | `engine = "starship"` |
| `ZSH_THEME=""` ou pas de `ZSH_THEME` | `engine = "none"` |
| `ZSH_THEME="random"` | `engine = "legacy"`, `theme = "robbyrussell"` (avertissement) |
| `ZSH_THEME="<nom>"` | `engine = "legacy"`, `theme = "<nom>"` |

Un plugin ou un thème introuvable donne un avertissement mais reste dans la configuration : il sera chargé dès qu'il sera installé, et en attendant le shell démarre normalement (le script contient un commentaire à sa place).

## Ce qui change dans le `.zshrc`

Avant toute modification, le fichier d'origine est copié dans `.zshrc.zsh-tool.bak` (ou `.zshrc.zsh-tool.<date>.bak` si cette sauvegarde existe déjà ; une sauvegarde n'est jamais écrasée). Ensuite :

- la ligne `source $ZSH/oh-my-zsh.sh` (ou `. ...`, `${ZSH}`, chemin littéral, guillemets) devient `eval "$(zsh-tool init zsh)"` ;
- les lignes `plugins=(...)` et `ZSH_THEME=...`, ainsi que la ligne `eval "$(starship init zsh)"` avec le moteur Starship, sont commentées avec le préfixe `# zsh-tool: ` ;
- `ZSH=...` et `ZSH_CUSTOM=...` deviennent `export ZSH=...` s'ils n'étaient pas exportés, car `zsh-tool init zsh` les lit dans son environnement ;
- tout le reste est conservé tel quel. Un `.zshrc` qui est un lien symbolique (gestionnaire de dotfiles) reste un lien : c'est sa cible qui est réécrite.

La migration refuse un `.zshrc` sans ligne `source .../oh-my-zsh.sh` reconnue, ou qui appelle déjà `zsh-tool init zsh` ; rien n'est alors écrit.

Exemple, pour le `.zshrc` du modèle Oh My Zsh :

```diff
-ZSH_THEME="robbyrussell"
+# zsh-tool: ZSH_THEME="robbyrussell"
-plugins=(git sudo z)
+# zsh-tool: plugins=(git sudo z)
-source $ZSH/oh-my-zsh.sh
+eval "$(zsh-tool init zsh)"
```

## Thèmes legacy et `lib/`

Avec `engine = "legacy"`, zsh-tool charge le fichier `<thème>.zsh-theme`, cherché dans cet ordre : `~/.local/share/zsh-tool/themes`, `$ZSH_CUSTOM/themes`, `$ZSH_CUSTOM`, `$ZSH/themes`. Le script active `prompt_subst` et les couleurs (`$fg`, `$fg_bold`, `$reset_color`, et `$FG`/`$BG`/`$FX` si le thème les utilise), puis fournit sa propre version des fonctions d'Oh My Zsh dont dépendent les thèmes, sans charger `lib/` :

- git : `git_prompt_info`, `parse_git_dirty`, `git_prompt_status`, `git_current_branch`, `git_remote_status`, `git_prompt_ahead`, `git_prompt_behind`, `git_prompt_remote`, `git_commits_ahead`, `git_commits_behind`, `git_prompt_short_sha`, `git_prompt_long_sha`, `git_current_user_name`, `git_current_user_email`, `git_repo_name`, `git_previous_branch` ; les réglages `ZSH_THEME_GIT_PROMPT_*`, `ZSH_THEME_GIT_SHOW_UPSTREAM`, `DISABLE_UNTRACKED_FILES_DIRTY`, `GIT_STATUS_IGNORE_SUBMODULES` et les options git `oh-my-zsh.hide-info`, `oh-my-zsh.hide-status`, `oh-my-zsh.hide-dirty` sont respectés ;
- `ruby_prompt_info`, `rvm_prompt_info`, et des fonctions vides (`virtualenv_prompt_info`, `nvm_prompt_info`, `hg_prompt_info`, etc.) que les plugins correspondants remplacent ;
- `ls` et `diff` colorés, `LSCOLORS`, `LS_COLORS` (sauf si `DISABLE_LS_COLORS=true`), `ZSH_THEME_TERM_TITLE_IDLE`.

Ces fonctions sont synchrones (pas de prompt asynchrone pour l'instant). Elles sont définies avant les plugins : un plugin qui fournit sa propre version la remplace.

Les autres réglages de `lib/` (historique, complétion, raccourcis clavier, titre du terminal, `take`, `omz`...) ne sont pas repris par défaut. Pour retrouver exactement le comportement d'Oh My Zsh, activer :

```toml
[compat]
lib = true
```

Tous les fichiers `$ZSH/lib/*.zsh` sont alors chargés dans l'ordre alphabétique, avant les plugins, comme le fait Oh My Zsh (un fichier de même nom dans `$ZSH_CUSTOM/lib` le remplace). Ils redéfinissent les fonctions de thème de zsh-tool par celles d'Oh My Zsh, y compris le prompt git asynchrone. Cette option ralentit le démarrage.

## Revenir en arrière

```sh
mv ~/.zshrc.zsh-tool.bak ~/.zshrc
```

`~/.config/zsh-tool/config.toml` peut être supprimé ou gardé : Oh My Zsh l'ignore.

## Limites connues

- Les fichiers `$ZSH_CUSTOM/*.zsh`, que Oh My Zsh charge après les plugins, ne sont pas chargés : les sourcer depuis le `.zshrc` si besoin.
- Les réglages `zstyle ':omz:...'` (désactivation des alias d'un plugin, mises à jour) et la mise à jour automatique d'Oh My Zsh ne s'appliquent plus.
- Seules les formes simples sont reconnues : une affectation ou un `source` par ligne. Un `plugins=(...)` construit dynamiquement, ou un `source` combiné à d'autres commandes sur la même ligne, est à adapter à la main.
