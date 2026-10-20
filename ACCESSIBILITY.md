# Accessibilité

ZER0│GH0ST est un **agent en terminal** (CLI + TUI). L'accessibilité ici ne parle pas de
page web : elle parle de **pilotage au clavier, de lisibilité, et de ne rien rendre
illisible quand le terminal n'est pas idéal**.

## Ce que le projet fait

- **Pilotage clavier complet** : aucune fonction n'exige la souris. Les raccourcis sont
  listés dans l'aide (`F5`) et dans le README.
- **La molette** fait défiler la conversation, les panneaux et les listes ; le **clic
  droit** déplie un bloc de sortie d'outil — mais tout a un équivalent clavier
  (`Ctrl+O`, `↑`/`↓`, `PgUp`/`PgDn`).
- **La couleur n'est jamais le seul canal** : les statuts sont doublés par du texte
  (`role:off`, `clé: oui/non`, `⚠`, `→`). Un daltonien, un terminal monochrome ou un
  lecteur d'écran restent informés.
- **Contraste** : thème sombre à contraste élevé, texte principal en blanc/gris clair.
- **Aucune animation obligatoire** : l'animation du banner est décorative et
  s'interrompt quand le travail est fini ; rien ne clignote de façon agressive.
- **Largeur de terminal** : l'interface se réorganise (le panneau latéral disparaît sous
  ~110 colonnes) ; les sorties d'outils sont repliées à 3 lignes pour rester lisibles.
- **Scriptable** : toute fonction est accessible en CLI (`--help`), donc utilisable dans
  un pipeline, avec un lecteur d'écran, ou par un wrapper.

## Limites connues (honnêteté)

- **La TUI ratatui/crossterm n'est pas conçue pour les lecteurs d'écran** : le rendu
  visuel (barres, blocs, couleurs) est difficile à vocaliser. L'**interface CLI** est
  la voie accessible (sortie texte, pipeable).
- **Pas d'équivalent web/GUI** : par choix (CLI-only, pas de serveur web).
- **Les sorties d'outils brutes** (code, logs) peuvent être longs : d'où le repli à
  3 lignes et `Ctrl+O` pour déplier — c'est un choix de lisibilité, pas un défaut.
- **Le presse-papier** utilise `wl-copy`/`xclip`/`pbcopy` : selon l'OS, le collage peut
  dépendre du presse-papier système.
- **Aucune Régulation (WCAG) formelle** n'a été auditée : ce document décrit l'effort et
  ses limites, ce n'est pas une certification.

## Signaler une barrière

Si quelque chose t'empêche d'utiliser le projet (contraste insuffisant, raccourci
inattendu, sortie inexploitable dans un lecteur d'écran, largeur minimale, etc.), ouvre
une *issue* « accessibilité » : c'est traité comme un **bug**, pas comme une demande
secondaire.

## Contribuer

Améliorer l'accessibilité est une contribution bienvenue. Points d'appui :

- ne jamais introduire d'information **uniquement** par la couleur ;
- garder un équivalent clavier pour toute action à la souris ;
- tester avec `NO_COLOR=1` et dans un terminal étroit (`COLUMNS=80`) ;
- documenter les nouveaux raccourcis dans l'aide (`F5`) et le README.
