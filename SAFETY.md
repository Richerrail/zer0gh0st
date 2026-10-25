# Zer0 — SAFETY.md (Sécurité et modèle de menace)

> **Règle fondamentale** : une propriété de sûreté non documentée n'est pas une propriété sûre —
> elle est invérifiable. Quelqu'un qui décide de lancer Zer0 ne dispose que de ces documents.

**Version** : 1.0 (2026-10-06) · **Projet** : 0.6.10

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

> ⚠️ **Responsabilité.** Zer0 agit avec **tes droits**, sans bac à sable. S'il exécute par erreur
> une commande destructrice — un `rm -rf` sur ton disque de photos de famille, un `dd` sur le
> mauvais disque, un `git push --force` — **c'est toi le responsable**, pas l'auteur du logiciel.
> Activer `code_exec` (`F4` dans la TUI, `/exec on` sur Telegram) est une **acceptation en bloc
> des risques** : tu le fais en connaissance de cause.

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
| **Passerelle `zer0-gw` (zero-knowledge)** | Les données transitent par un dossier distant. | Le mode zero-knowledge chiffre de bout en bout (le dossier distant ne déchiffre jamais) ; en mode standard/anonyme la confiance au dossier distant est requise. La passphrase n'est jamais stockée. |

---

## 3. Ce qui protège — et ce qui ne protège pas

**Protections réelles :**
- `F3`/`F4` : outils désactivés par défaut → l'agent ne peut pas agir sans activation explicite.
- **Journal d'audit** (`data/audit.jsonl`, append-only, rotation à 8 Mio) : chaque appel
  d'outil est enregistré — arguments **complets** (c'est la trace vérifiable), résultat
  résumé, horodatage, dossier courant, sous-agents `0box` compris. Lecture : `zer0 audit`
  ou `/audit`. Auditer n'empêche rien : ça rend l'action relisible après coup.
- **Sauvegarde avant écrasement** : `text_editor` copie le fichier en `<nom>.zer0.bak`
  avant de le réécrire ou de le supprimer. Une seule génération conservée — ce n'est pas
  un historique, seulement un filet contre l'erreur immédiate du modèle.
- **Budget d'appels LLM par tour** (120 par défaut, `ZER0_MAX_LLM_CALLS`), **partagé par
  les sous-agents** : un `0box` récursif ne peut plus vider un quota en un message.
- **Telegram** : `code_exec` **et** `text_editor` sont derrière `/exec` ; `/exec on` est
  refusé sur un chat lié automatiquement (auto-claim), sauf `ZER0_TELEGRAM_ALLOW_EXEC=1`.
- **Passerelle `zer0-gw`** : endpoints `/admin/*` protégés par un jeton (`admin_token`
  dans `gw.toml` ou `ZER0_GW_ADMIN_TOKEN`, en-tête `x-zer0-admin`). Sans jeton, tout
  processus local peut remplacer ou supprimer une clé : l'avertissement est journalisé
  au démarrage, et l'exemple `deploy/gw.example.toml` documente la génération du jeton.
- `text_editor create` **refuse** un contenu vide (pas de fichier fantôme).
- Contrôle du raisonnement (`reasoning.enabled`) et plafonds de flux (1 Mo / 120 s) contre les boucles.
- Coffre (Argon2 + ChaCha20-Poly1305) : clés API **chiffrées au repos**, `chmod 600`.
- Mémoire **non auto-injectée** dans le modèle (toutes les entrées `untrusted`).

**Ne protège pas :**
- Contre une commande `code_exec` destructrice mais valide (le journal d'audit la
  documentera, il ne l'empêchera pas).
- Contre un écrasement en cascade : `.zer0.bak` garde **une** version, pas l'historique.
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

---

## 7. Réduire l'exfiltration : `zer0-gw` + egress (déploiement durci)

Le coffre chiffre les clés **au repos** ; tant que la clé est résolue dans le processus
agent (`config::key_for`), un agent compromis peut la lire et l'envoyer ailleurs.
Le déploiement durci retire la clé du processus — quatre couches, indépendantes :

1. **Passerelle `zer0-gw`** (optionnel : `cargo build --release --features gateway`) :
   le seul processus qui détient les clés (`/etc/zer0-gw/`, lisible par l'utilisateur
   `zer0-gw` uniquement). L'agent n'appelle que `http://127.0.0.1:8787` **sans aucune
   clé** ; la passerelle injecte l'en-tête `Authorization` vers le provider. Exfiltrer
   une clé depuis l'agent devient impossible — elle n'y a jamais été.
2. **Egress nftables** (`deploy/20-zer0-egress.nft`) : l'utilisateur `zer0` ne peut
   joindre que le loopback ; seul `zer0-gw` sort, en TCP 443. Une exfiltration devient
   un refus réseau, pas une affaire de confiance.
3. **Unités systemd durcies** (`deploy/*.service`) : utilisateurs dédiés,
   `ProtectSystem`, `SystemCallFilter`, et surtout `IPAddressAllow=localhost` pour
   l'agent headless — le blocage le plus fort, sans même nftables.
4. **Hygiène d'environnement** (`platform::scrub_secrets`) : les variables ressemblant
   à des secrets (`*API_KEY*`, `*TOKEN*`, `*PASSPHRASE*`, `ZER0_VAULT*`, …) sont
   retirées de l'environnement de tout processus enfant — `code_exec` ne peut plus
   faire `env` pour récupérer une clé héritée.

Installation : `sudo scripts/gw-setup.sh`. Côté agent, plus aucune clé à saisir :
`/provider add openrouter sk-…` (TUI) ou `zer0 provider add …` (CLI) **déposent la clé
sur la passerelle** via un endpoint admin loopback **en écriture seule** (on peut
déposer, jamais relire), puis basculent l'agent sur `gw-<id>`. `/provider keys` affiche
l'état des clés détenues par `zer0-gw`. Si la passerelle est absente, repli local avec
avertissement — le coffre/`chmod 600` du §4 s'applique alors, et non le durcissement.

**Ce que ça ne protège pas** : l'agent compromis peut toujours **utiliser le relais**
(demander à `zer0-gw` d'appeler un provider, au détriment de votre quota — d'où le
rate-limit côté passerelle et les plafonds de dépense côté provider) ; il voit toujours
tout ce qui passe dans son contexte. Un processus qui exécute du code arbitraire n'est
pas devenu digne de confiance : **sa clé est devenue inatteignable, c'est différent**.
Les recommandations du §6 (VM/conteneur, pas de session authentifiée) restent valables.

Honnêteté : la passerelle est un binaire de ~450 lignes avec liste blanche de chemins,
rate-limit, refus des redirections et journal sans clés — mais elle n'a pas été auditée
indépendamment. Elle réduit la surface d'attaque, elle ne prétend pas la fermer.
