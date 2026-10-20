# Politique de sécurité

## Le modèle de menace d'abord

ZER0│GH0ST exécute du code arbitraire : `code_exec` lance un shell **avec tes droits**,
sans bac à sable. Ce n'est pas un détail — c'est le cœur du projet, et c'est assumé.

La politique complète est dans **[SAFETY.md](./SAFETY.md)** : modèle de menace, ce que le
projet **protège**, et ce qu'il **ne protège pas** (filtrage des commandes, exfiltration
par le processus, injections via le web, conditions des fournisseurs). Lis-le avant
d'utiliser l'agent sur des données sensibles.

Le point clé du découpage actuel : **les clés API ne sont plus dans le processus agent**.
Elles sont détenues par `zer0-gw` (utilisateur dédié, `/etc/zer0-gw/`), déposées en
écriture seule, et l'agent ne les résout jamais.

## Signaler une vulnérabilité

**N'ouvre pas d'issue publique** pour une faille.

- **Preferred** : *Security* → *Report a vulnerability* sur
  <https://github.com/Richerrail/zer0gh0st/security/advisories/new> (ouvre une
  *security advisory* privée).
- **Sinon** : e-mail au maintainer (`richerrail@gmail.com`).

Inclure, si possible :

- la version affected (`zer0 --version`) et la plateforme ;
- les étapes de reproduction (une commande suffit) ;
- l'impact attendu ;
- si tu as déjà Publish ou non.

## Délai

- **Accusé de réception** : sous 7 jours.
- **Évaluation** : sous 30 jours (ou explication du délai).
- **Correctif** : selon la gravité — critique : aussi vite que possible ; sinon : la
  prochaine version.

## Divulgation

Merci de ne pas diffuser publiquement avant qu'un correctif soit disponible. Une fois le
correctif publié, tu es libre de le mentionner, et ta contribution sera créditée
(sauf si tu préfères rester anonyme).

## Périmètre

| Inclus | Hors périmètre |
|---|---|
| `code_exec` / `text_editor` exécutant des actions non attendues | l'absence de bac à sable (choix assumé, documenté) |
| fuite de clé via le processus agent | le modèle qui se trompe (c'est un LLM) |
| `zer0-gw` (endpoints admin, validation SHA256, fichier de clés) | la disponibilité des providers |
| le vault (`~/.config/zer0/vault.enc`) | les conditions d'utilisation d'un fournisseur tiers |
| la TUI (injection de commande, exécution de code arbitraire) | les accès de l'utilisateur à ses propres fichiers |

## Durcissement disponible

Pour réduire la surface d'attaque : `sudo scripts/gw-setup.sh` installe la passerelle
`zer0-gw`, les règles d'egress (nftables) et des unités systemd durcies. Voir
[SAFETY.md](./SAFETY.md) §7.
