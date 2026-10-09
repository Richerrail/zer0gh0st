# Zer0 — SAFETY.md (Sécurité et modèle de menace)

> **Règle fondamentale** : une propriété de sûreté non documentée n'est pas une propriété sûre —
> elle est invérifiable. Quelqu'un qui décide de lancer Zer0 ne dispose que de ces documents.

**Version** : 1.0 (2026-10-06) · **Projet** : 0.5

---

## 1. Choix de conception assumé

Zer0 n'a **pas de bac à sable**. Avec `code_exec` activé (`F4`) et `text_editor` (`F3`), l'agent
dispose des **droits de l'utilisateur** : exécuter des commandes, lire/écrire/supprimer des fichiers.

**Le choix explicite de ce projet est : Option B assumée + Option C recommandée.**

- **Option A (interdire)** — filtrer/refuser les commandes dangereuses : **non retenue pour l'instant**
  (aucune blocklist n'est implémentée ; filtrer l'intention derrière `bash` est illusoire).
- **Option B (assumer)** — Zer0 s'exécute avec tous les droits. **Utiliser dans une VM/conteneur.**
  Mais une VM ne protège que l'hôte : elle n'empêche pas l'agent d'exfiltrer, depuis l'intérieur,
  les secrets auxquels il a accès.
- **Option C (recommandée)** — séparer les **secrets** du bac à sable : pas de coffre déverrouillé
  ni de session authentifiée là où tourne l'agent.

Le document n'impose pas C ; il exige que vous le sachiez et le décidiez.

---

## 2. Modèle de menace (ce que ce document couvre)

| Vecteur | Description | Ce que le document dit |
|---|---|---|
| **Escalade destructive** | Un prompt ambigu peut produire un `rm -rf` sans clarification. | `code_exec` exécute `bash -lc` tel quel, **sans filtre**. Un conteneur limite la casse, pas la sortie. |
| **Injection via recherche web** | Un résultat `web_search` contenant des instructions peut orienter l'agent. | Vecteur **par construction** (`web_search` + `code_exec` + obéissance du modèle). Non mesuré. |
| **Chaîne d'outils** | Un contenu écrit par un outil peut être relu et servir d'instruction. | `memory_search` renvoie toujours `trust_enforced: false` → traité comme **donnée**, pas instruction. |
| **Coffre dans le même processus** | `text_editor` lit un fichier ; `code_exec` peut le faire sortir. | Le coffre protège le **disque au repos**, pas l'exfiltration par le processus. |
| **Data path provider** | Prompts et réponses passent par le provider (et son gateway). | Rétention/logging = termes du provider. Ne pas router de données sensibles. |
| **Modèles `:free` gated** | Un modèle peut être réservé à des apps whitelistées (403). | Non prévisible (absent du catalogue) → bascule automatique, jamais un mensonge sur la fiabilité. |

---

## 3. Ce qui protège — et ce qui ne protège pas

**Protections réelles :**
- `F3`/`F4` : outils désactivés par défaut → l'agent ne peut pas agir sans activation explicite.
- `text_editor create` **refuse** un contenu vide (pas de fichier fantôme).
- Contrôle du raisonnement (`reasoning.enabled`) et plafonds de flux (1 Mo / 120 s) contre les boucles.
- Coffre (Argon2 + ChaCha20-Poly1305) : clés API **chiffrées au repos**, `chmod 600`.
- Mémoire **non auto-injectée** dans le modèle (toutes les entrées `untrusted`).

**Ne protège pas :**
- Contre une commande `code_exec` destructrice mais valide.
- Contre l'exfiltration d'un secret lu dans le processus.
- Contre l'injection de prompt via du texte récupéré.
- Contre les provider/gateways (rétention, geo, conditions d'utilisation).

---

## 4. Clés et secrets

| Stockage | Protection |
|---|---|
| Variable d'env (`OPENROUTER_API_KEY=…`) | recommandé : ne touche pas le disque |
| `~/.config/zer0/config.toml` (en clair) | `chmod 600` uniquement |
| `~/.config/zer0/vault.enc` | chiffré, déverrouillé par passphrase (`$ZER0_VAULT_PASSPHRASE` ou prompt) |

> Passez au coffre : `zer0 vault setup` (retire les clés du `config.toml`).

---

## 5. Écosystème (`mods/`)

`/kristal`, `/streaming`, `/retro` lancent des programmes **tiers** copiés dans `mods/`
(Qt6, Python/libtorrent, HTML/JS). Ils ont leurs propres accès et ne sont **pas** couverts par
ce modèle de menace. Voir [`ECOSYSTEM.md`](./ECOSYSTEM.md).

---

## 6. Recommandation d'usage

1. Tourner Zer0 dans une **VM/conteneur** dédié.
2. Ne **pas** y déverrouiller le coffre ni y ouvrir de session authentifiée si l'agent traite du web.
3. Activer `F4` (code_exec) **seulement** quand nécessaire.
4. Souscrire aux conditions de chaque provider ; ne pas router de données sensibles.
