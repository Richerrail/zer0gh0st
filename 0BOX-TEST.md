# 0BOX-TEST — projet test pour valider le module `0box`

**But** : valider que `0box` (brainstorming multi-agents sur une tâche) sait
**dispatcher les tâches entre les 8 rôles**, faire respecter les dépendances, et
faire valider le travail par le rôle `review`.

**Projet test : « compteur »** — un mini-outil CLI Python qui analyse un fichier
texte/JSONL et produit un rapport de statistiques (lignes, mots, top-N mots).

- **Volontairement petit** : terminable en une session.
- **Volontairement complet** : sollicite les **8 rôles**, avec livrables
  **vérifiables sur disque** (0box et `review` peuvent contrôler objectivement).

---

## 1. Les 8 rôles et leur tâche

| ID | Rôle | Livrable | Dépend de |
|----|-----------|---------------------------------------------|-----------|
| t5 | `main` | `~/0box-test/RAPPORT.md` (coordination) | — |
| t6 | `reasoning` | `~/0box-test/notes_conception.md` | — |
| t7 | `vision` | `~/0box-test/mockup_analyse.md` | fixture |
| t8 | `fast` | `~/0box-test/reponses_rapides.md` | — |
| t9 | `utility` | `sample.txt`, `sample.jsonl`, `sample.csv` | — |
| t10 | `code` | `compteur.py` + `test_compteur.py` (tests exécutés) | t6, t7, t9 |
| t11 | `writing` | `~/0box-test/README.md` (français) | t10 |
| t12 | `review` | `~/0box-test/REVUE.md` + **verdict APPROUVÉ/REJETÉ** | t5→t11 |

Fixture fournie : `~/0box-test/mockup.txt` (mockup ASCII du rapport — voir §3).

## 2. Recréer les tâches (sur n'importe quelle machine)

Depuis la racine du projet (les tâches vivent dans `.zer0/tasks.md`) :

```bash
zer0 task add "[0box-test] Coordination du projet « compteur » : suivre t6→t12, débloquer les rôles en attente, intégrer les livrables dans ~/0box-test/, tenir un rapport d'avancement → ~/0box-test/RAPPORT.md" --role main
zer0 task add "[0box-test] Concevoir « compteur » (CLI Python) : entrées .txt/.jsonl, cas limites (fichier vide, encodage, très gros fichier), format de sortie txt|json, arbitrages → ~/0box-test/notes_conception.md" --role reasoning
zer0 task add "[0box-test] Lire le mockup ASCII ~/0box-test/mockup.txt (rendu du rapport) et lister précisément les éléments UI à implémenter (champs, barres, alignements) → ~/0box-test/mockup_analyse.md" --role vision
zer0 task add "[0box-test] Réponses courtes (5 lignes max chacune) : 1) « compteur » peut-il être stdlib-only ? 2) JSON ou CSV pour l'export ? 3) pytest ou unittest ? → ~/0box-test/reponses_rapides.md" --role fast
zer0 task add "[0box-test] Générer les jeux de test : ~/0box-test/sample.txt (~300 mots avec mots répétés), ~/0box-test/sample.jsonl (20 lignes {\"texte\": \"…\"}) et ~/0box-test/sample.csv (conversion du jsonl)" --role utility
zer0 task add "[0box-test] Implémenter ~/0box-test/compteur.py (argparse : fichier, --format txt|json, --top N) + ~/0box-test/test_compteur.py (couvre les cas limites de t6) ; EXÉCUTER les tests et coller le résultat → après t6, t7, t9" --role code
zer0 task add "[0box-test] Rédiger ~/0box-test/README.md en français : install, usage, exemples de sortie, limites (honnêtes) → après t10" --role writing
zer0 task add "[0box-test] Revue finale du projet « compteur » : checklist (tous les livrables présents dans ~/0box-test/, tests verts, README complet, pas de fichiers temporaires oubliés) puis verdict final « APPROUVÉ » ou « REJETÉ » motivé → ~/0box-test/REVUE.md — EN DERNIER, après t5→t11" --role review
```

> Les IDs réels dépendent des tâches déjà présentes (`t1`, `t2`, …) — adapter
> les références « après t6 » en conséquence.

## 3. Fixture : mockup ASCII (pour le rôle `vision`)

```bash
mkdir -p ~/0box-test
```

Écrire `~/0box-test/mockup.txt` :

```
Mockup ASCII — rendu attendu du rapport « compteur »
=====================================================
┌──────────────────────────────────────────────────┐
│  compteur — rapport                              │
├──────────────────────────────────────────────────┤
│  Fichier     : sample.txt                        │
│  Lignes      : 42                                │
│  Mots        : 318                               │
│  Caractères  : 1904                              │
├──────────────────────────────────────────────────┤
│  Top mots                                        │
│   1. zer0      ████████████████  12              │
│   2. agent     ██████████  8                     │
│   3. ghost     ████████  6                       │
│   ...                                            │
├──────────────────────────────────────────────────┤
│  [--format json] → même contenu, sortie JSON     │
└──────────────────────────────────────────────────┘
```

> **Honnêteté** : ZER0v1 n'a **pas d'entrée image** (les captures d'écran ne
> sont pas envoyées au modèle). Le rôle `vision` est donc testé en **lecture de
> schéma ASCII** — c'est son vrai périmètre actuel, pas une démo trompeuse.

## 4. Critères de réussite de 0box

1. **Dispatch correct** : chaque tâche est envoyée au rôle marqué dans la
   tâche — pas tout à `main`, pas de rôle inventé (les 8 rôles valides :
   `main, code, reasoning, writing, vision, fast, utility, review`).
2. **Dépendances respectées** : `code` (t10) ne démarre pas avant
   `reasoning`/`vision`/`utility` ; `review` (t12) est **en dernier**.
3. **Livrables réels** : chaque livrable existe sur disque, non vide
   (`wc -c`, ou lecture) — 0box ne doit pas accepter « j'ai écrit le fichier »
   sans vérification.
4. **Tests exécutés** : `python3 -m pytest ~/0box-test/test_compteur.py`
   (ou unittest) passe — le résultat est collé, pas affirmé.
5. **Verdict de review** : `REVUE.md` se termine par **« APPROUVÉ »** ou
   **« REJETÉ »** avec les raisons. Un review qui approuve alors qu'il manque
   un livrable = échec du test.
6. **Pas de pollution** : pas de fichiers temporaires oubliés dans
   `~/0box-test/` (la consigne système l'exige).

## 5. Vérification après coup

```bash
ls -la ~/0box-test/            # tous les livrables présents ?
python3 ~/0box-test/compteur.py ~/0box-test/sample.txt            # rapport txt
python3 ~/0box-test/compteur.py ~/0box-test/sample.jsonl --format json
tail -5 ~/0box-test/REVUE.md   # verdict ?
```

## 6. Nettoyage (fin du test)

```bash
zer0 task rm t5   # …t12   (ou garder pour re-tester)
rm -rf ~/0box-test
```
