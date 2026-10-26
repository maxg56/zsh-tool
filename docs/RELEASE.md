# Publier une version

Procédure du mainteneur pour publier la version `X.Y.Z` de zsh-tool : crates.io, release GitHub (binaires et formule Homebrew) et tap Homebrew. Les exemples utilisent `0.1.0`.

## Ce que font les workflows

### CI (`.github/workflows/ci.yml`)

Lancée à chaque push sur `main` et sur chaque pull request, avec `RUSTFLAGS=-D warnings` :

- job **Test**, sous `ubuntu-latest` et `macos-latest` : installation de zsh (Linux), `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, puis `cargo test` avec `ZSH_TOOL_REQUIRE_ZSH=1` (les tests d'intégration échouent si zsh est absent au lieu d'être sautés) ;
- job **Minimum Rust version** : lit `rust-version` dans `Cargo.toml` et lance `cargo check --locked` avec cette version de Rust.

### Release (`.github/workflows/release.yml`)

Lancée par le push d'un tag `v*`. Elle ne lance pas les tests : c'est la CI de `main` qui les garantit.

1. Job **Build**, une fois par cible :

   | Cible | Machine |
   | --- | --- |
   | `x86_64-unknown-linux-musl` | `ubuntu-latest` |
   | `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` |
   | `x86_64-apple-darwin` | `macos-latest` |
   | `aarch64-apple-darwin` | `macos-latest` |

   Pour chaque cible : `cargo build --release --locked --target <cible>` (avec `musl-tools` sous Linux), puis, sous Linux, vérification avec `file` que le binaire est statique (`statically linked` ou `static-pie linked`) ; le job échoue sinon. Le binaire, `LICENSE` et `README.md` sont placés dans un dossier `zsh-tool-<tag>-<cible>/`, archivé en `zsh-tool-<tag>-<cible>.tar.gz`, avec sa somme `zsh-tool-<tag>-<cible>.tar.gz.sha256` (`shasum -a 256`). `fail-fast` est activé : une cible en échec annule les autres.

2. Job **Publish GitHub release**, après les quatre builds : il récupère les archives et les sommes, génère la formule Homebrew `zsh-tool.rb` avec `packaging/homebrew/generate.sh <tag> dist` (une URL et une somme SHA-256 par plateforme, dépendance `zsh`, test `zsh-tool --version` et `zsh-tool init zsh`), puis crée la release avec `gh release create <tag> dist/* --title <tag> --generate-notes --verify-tag`. Les notes sont générées par GitHub à partir des pull requests fusionnées depuis le tag précédent.

La release contient donc 9 fichiers : 4 archives, 4 sommes SHA-256 et `zsh-tool.rb`.

Un premier job, **Check tag against Cargo.toml**, arrête le workflow si le tag n'est pas exactement `v` suivi de la version de `Cargo.toml`. Le workflow ne publie rien sur crates.io : c'est à la charge du mainteneur, après une release réussie.

## Procédure

### 1. Vérifier `main`

```sh
git switch main
git pull --ff-only
gh run list --branch main --workflow CI --limit 1   # le dernier run doit être « success »
```

Ne pas publier si la CI du dernier commit de `main` n'est pas verte.

### 2. Fixer la version

Dans `Cargo.toml`, `version = "X.Y.Z"`. Mettre ensuite `Cargo.lock` à jour (le build de release utilise `--locked` et échoue si le lock n'est pas à jour), puis vérifier localement :

```sh
cargo build --release
cargo fmt --check
cargo clippy --all-targets -- -D warnings
ZSH_TOOL_REQUIRE_ZSH=1 cargo test
```

Mettre à jour les exemples d'installation de `README.md` et `README.fr.md` (`version=vX.Y.Z`, URL de `zsh-tool.rb`), puis committer `Cargo.toml`, `Cargo.lock` et les README dans une pull request vers `main`, et attendre que la CI soit verte après fusion. Pour la première version, `0.1.0` est déjà en place.

### 3. Pousser le tag

Sur le commit de `main` à publier :

```sh
git tag -a v0.1.0 -m "zsh-tool 0.1.0"
git push origin v0.1.0
gh run watch "$(gh run list --workflow Release --limit 1 --json databaseId --jq '.[0].databaseId')"
```

Le tag doit commencer par `v` et correspondre exactement à la version de `Cargo.toml` (`v0.1.0` pour `0.1.0`) : c'est lui qui donne son nom aux archives et sa version à la formule, et le workflow refuse un tag qui ne correspond pas.

Vérifier ensuite la release :

```sh
gh release view v0.1.0
gh release download v0.1.0 --dir /tmp/zsh-tool-v0.1.0
cd /tmp/zsh-tool-v0.1.0 && for f in *.sha256; do shasum -a 256 -c "$f"; done
```

Si le workflow échoue avant la création de la release, corriger le problème sur `main`, supprimer le tag (`git push --delete origin v0.1.0`, `git tag -d v0.1.0`) et reprendre à l'étape 3 sur le nouveau commit. Rien n'est encore publié sur crates.io à ce stade, ce qui rend l'opération sans risque.

### 4. Publier sur crates.io

Seulement une fois la release GitHub réussie. Une version publiée sur crates.io ne peut plus être remplacée (seulement retirée avec `cargo yank`) : toujours faire un essai d'abord. Il faut un jeton crates.io (`cargo login`) et un dépôt local propre, sur le commit du tag (`git switch --detach vX.Y.Z`).

```sh
cargo package --list          # fichiers envoyés : sources, README.md, LICENSE, docs
cargo publish --dry-run       # construit le paquet et le compile, sans rien envoyer
cargo publish
```

Le paquet exclut `.github`, `scripts`, `packaging` et `tests` (voir `exclude` dans `Cargo.toml`) : les tests ne sont pas lancés depuis crates.io et leurs fixtures contiennent une copie partielle d'Oh My Zsh.

### 5. Publier la formule dans le tap Homebrew

La formule jointe à la release peut être installée telle quelle (`brew install --formula ./zsh-tool.rb`), mais un tap permet `brew install maxg56/tap/zsh-tool` et `brew upgrade`.

Création du tap, une seule fois : créer sur GitHub le dépôt public `maxg56/homebrew-tap` (le préfixe `homebrew-` est obligatoire ; Homebrew l'appelle alors `maxg56/tap`), avec un dossier `Formula/`.

```sh
gh repo create maxg56/homebrew-tap --public --description "Homebrew formulae for zsh-tool"
git clone https://github.com/maxg56/homebrew-tap.git
mkdir -p homebrew-tap/Formula
```

À chaque version, copier la formule de la release dans le tap :

```sh
cd homebrew-tap
gh release download v0.1.0 --repo maxg56/zsh-tool --pattern zsh-tool.rb --dir Formula --clobber
git add Formula/zsh-tool.rb
git commit -m "zsh-tool 0.1.0"
git push
```

Puis vérifier sur une machine avec Homebrew (macOS, ou Linux avec Homebrew) :

```sh
brew update
brew install maxg56/tap/zsh-tool     # ou brew upgrade zsh-tool
brew test zsh-tool
zsh-tool --version
```

Une fois le tap en place, les sections Homebrew des README peuvent indiquer `brew install maxg56/tap/zsh-tool` comme méthode principale.

## Liste de contrôle

- [ ] CI verte sur le dernier commit de `main`
- [ ] `version` de `Cargo.toml` et `Cargo.lock` à `X.Y.Z`, exemples des README à jour
- [ ] Tag `vX.Y.Z` poussé, workflow Release terminé, 9 fichiers dans la release, sommes vérifiées
- [ ] `cargo publish --dry-run` puis `cargo publish`, sur le commit du tag
- [ ] `Formula/zsh-tool.rb` mis à jour dans `maxg56/homebrew-tap`, `brew install` et `brew test` réussis
