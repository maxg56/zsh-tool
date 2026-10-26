# Mesures de démarrage

Mesures du jalon J1, obtenues avec `scripts/bench.sh`, qui crée deux dossiers personnels isolés avec les mêmes plugins et compare `zsh -i -c exit` sous Oh My Zsh et sous zsh-tool.

## Reproduire

```sh
git clone --depth 1 https://github.com/ohmyzsh/ohmyzsh.git /tmp/omz
RUNS=50 scripts/bench.sh /tmp/omz                  # 10 plugins par défaut
scripts/bench.sh /tmp/omz git sudo z               # liste personnalisée
```

Le script demande `zsh` et `hyperfine`, compile zsh-tool en mode release et fait trois exécutions de chauffe pour remplir les caches (fichier `zcompdump`, script d'init) avant de mesurer.

Pour profiler un shell plugin par plugin, ajouter `zmodload zsh/zprof` en tête du `.zshrc` et `zprof` à la fin, puis lancer `zsh -i -c exit`.

## Résultats du 26 octobre 2026

Machine : conteneur Linux x86_64, 4 vCPU (Intel Xeon @ 2,80 GHz), zsh 5.9, Oh My Zsh `d2b7f55`, aucun thème.
Plugins : git, sudo, z, extract, colored-man-pages, history, docker, kubectl, npm, python.

| Commande | Moyenne (ms) | Écart |
|:---|---:|---:|
| Oh My Zsh, 10 plugins | 65,4 ± 10,1 | |
| zsh-tool, 10 plugins | 46,8 ± 5,7 | −28 % |
| `zsh-tool init zsh` (cache valide) | 4,1 ± 0,5 | |

`zsh-tool init zsh` avec cache valide reste sous la cible de 10 ms (ENF1). Le gain sur le shell complet est modeste à ce stade, car zsh-tool source encore les mêmes fichiers `*.plugin.zsh` que Oh My Zsh ; l'essentiel du gain viendra des plugins natifs et du chargement paresseux (jalons J3 et J5). Ces chiffres viennent d'un conteneur partagé et servent de référence de départ, pas de résultat final : le rapport de J5 refera la mesure sur la machine de référence.
