# ZER0v1 — Quelles commandes slash de `zero.py` porter ?

Analyse : `~/zer0/zero.py` (30 commandes slash).
Cible : **ZER0v1** (agent Rust, CLI-only).

Légende : 🟢 à porter · 🟡 optionnel · ⚪ déjà là · ❌ non pertinent.

---

## 1. Déjà présent dans ZER0v1 ⚪

| zero.py | ZER0v1 |
|---|---|
| `/think on/off` | `/think` ✅ |
| `/task` | `/task`, `/work` ✅ |
| `/context clear/compact` | `/clear`, `/compact` ✅ |
| `/menu` | `/help` ✅ |
| `/fast on/off` | `/provider`, `/model`, `/route` ✅ |

---

## 2. À porter — priorité haute 🟢

| Commande | Ce qu'elle fait | Pourquoi pour ZER0v1 | Effort |
|---|---|---|---|
| **`/duck` (web_search)** | recherche DuckDuckGo | **ZER0v1 n'a AUCUN accès web** — un vrai manque. Ajouter l'outil `web_search` + `/search` | moyen |
| **`/vault`** | coffre de secrets (status/setup/reset/passphrase) | on a vu que ZER0v1 stocke les clés API **en clair** ; un vault corrige ça | moyen |

---

## 3. À porter — priorité moyenne 🟡

| Commande | Ce qu'elle fait | Intérêt | Effort |
|---|---|---|---|
| **`/stop`** | interrompt la génération en cours | ZER0v1 n'a que Ctrl+C (quitte). Indispensable pour un long tour | petit |
| **`/context status`** | état de la mémoire conversation | complète `/clear` + `/compact` | minuscule |
| **`/whoami {0-4}`** | personas = prompts système alternatifs | ZER0v1 a des *rôles* (modèle) mais pas de personas (ton/prompt) | petit |
| **`/projet <nom>`** | charge un prompt de projet depuis une bibliothèque | ZER0v1 a `.zer0/project.toml` mais pas de bibliothèque de prompts | petit |

---

## 4. Optionnel / niche 🟡

| Commande | Intérêt |
|---|---|
| `/jev` | validation calibrée après code — ZER0v1 a déjà `/review` (verdict) ; Jev = score |
| `/terminal` | terminal interactif intégré — ZER0v1 a `code_exec`, mais pas de session interactive |
| `/pdf <cmd>` | outils PDF locaux (info/text/merge/split…) — niche mais propre |
| `/type_here` | contrôle GUI — ZER0v1 est volontairement CLI-only |

---

## 5. Non pertinent ❌ (écosystème Python Zer0)

`phone`, `funbox`, `radio`, `bintime`, `zer0-screen`, `kristal`, `retro`, `streaming`,
`edit`, `chan`, `cod`, `shell` (lance zer0shell), `ide`, `chat`, `telegram`, `discord`,
`asm`, `zer0net`, `formation`, `zeroFine`, `zeroKaggle`, `prof`, `db`, `adddb`, `rebuild-fts`.

→ GUI, médias, réseaux sociaux, entraînement/fine-tuning, base SQLite.
ZER0v1 est un agent **terminal multi-providers**, pas un écosystème desktop.

---

## 6. Roadmap de portage proposée (ZER0v1)

| Phase | Contenu | Effort | Statut |
|---|---|---|---|
| 1 | **`web_search`** : outil + `/search` (DDG, sans clé) | moyen | ✅ |
| 2 | **`/stop`** : annulation du tour (flag partagé) | petit | ✅ |
| 3 | **`/context`** : status (tokens, messages, rôle, endpoint) | minuscule | ✅ |
| 4 | **`/whoami`** : personas (fichiers de prompt) | petit | ✅ |
| 5 | **`/vault`** : chiffrement des clés (Argon2 + ChaCha20-Poly1305) | moyen | ✅ |
| 6 | **`/pdf`** (info/text/merge/split) + **`/jev`** (score 0-3) | moyen | ✅ |

**Tout est implémenté dans ZER0v1** (aucun lien vers `~/.local/share/zer0`).

---

## 7. Détail rapide : pourquoi `web_search` d'abord

- ZER0v1 répond uniquement depuis le modèle ; aucune vérification factuelle possible.
- Un outil `web_search` (DDG HTML) débloque : recherche, vérification, actualité,
  et alimente `memory_save`.
- Zéro clé API, zéro dépendance lourde (reqwest + parsing HTML minimal déjà dispo).
