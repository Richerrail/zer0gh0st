# Module email zer0gh0st

Lecture, recherche, envoi et gestion d'emails via IMAP et SMTP, exposés à
l'agent sous forme d'outil `email` unique.

## Architecture

```
src/email/
├── mod.rs          — point d'entrée
├── config.rs       — EmailConfig (variables d'environnement)
├── error.rs        — EmailError typé (thiserror)
├── types.rs        — EmailSummary, EmailMessage, SendRequest, EmailAction
├── imap_client.rs  — ImapClient (read/manage, TLS, UID)
├── smtp_client.rs  — SmtpClient (write, lettre, TLS/STARTTLS)
├── parser.rs       — parsing MIME (mail-parser)
└── service.rs      — façade EmailService (permissions + audit)
src/tools/email.rs  — outil `email` (JSON → EmailAction)
```

## Configuration

Toutes les variables sont requises sauf mention contraire. Aucun secret n'est
jamais loggué ni stocké en clair.

| Variable | Description | Défaut |
|---|---|---|
| `ZER0_EMAIL_IMAP_HOST` | Serveur IMAP | — |
| `ZER0_EMAIL_IMAP_PORT` | Port IMAP | `993` |
| `ZER0_EMAIL_IMAP_USER` | Utilisateur IMAP | — |
| `ZER0_EMAIL_IMAP_PASSWORD` | Mot de passe IMAP | — |
| `ZER0_EMAIL_SMTP_HOST` | Serveur SMTP | — |
| `ZER0_EMAIL_SMTP_PORT` | Port SMTP | `587` |
| `ZER0_EMAIL_SMTP_USER` | Utilisateur SMTP | — |
| `ZER0_EMAIL_SMTP_PASSWORD` | Mot de passe SMTP | — |
| `ZER0_EMAIL_FROM_NAME` | Nom d'affichage de l'expéditeur | — |
| `ZER0_EMAIL_FROM_ADDRESS` | Adresse expéditeur | — |
| `ZER0_EMAIL_ALLOW_READ` | Permission lecture | `false` |
| `ZER0_EMAIL_ALLOW_WRITE` | Permission envoi | `false` |
| `ZER0_EMAIL_ALLOW_MANAGE` | Permission flags/déplacement | `false` |
| `ZER0_EMAIL_ALLOW_DELETE` | Permission suppression | `false` |
| `ZER0_EMAIL_AUDIT_DIR` | Répertoire du journal d'audit | `/tmp` |

## Permissions

Chaque action vérifie sa permission **avant** toute connexion réseau :

| Permission | Actions |
|---|---|
| `allow_read` | `list_folders`, `list_messages`, `get_message`, `search` |
| `allow_write` | `send`, `reply`, `forward` |
| `allow_manage` | `mark_read`, `move` |
| `allow_delete` | `delete` |

Un refus est journalisé dans l'audit (`permission_denied`) et renvoyé à
l'agent comme erreur.

## Audit

Chaque action (réussie ou refusée) écrit une ligne JSON dans
`$ZER0_EMAIL_AUDIT_DIR/email_audit.log` : timestamp, action, détails
(mailbox, uid, compteurs). **Jamais** le contenu des messages ni les secrets.

## Sécurité

- Connexions TLS uniquement : IMAP sur 993 (TLS implicite), SMTP sur 465
  (TLS implicite) ou 587 (STARTTLS obligatoire).
- Suppression en deux temps : `delete` marque `\Deleted` ; l'effacement
  définitif exige `expunge: true` explicite.
- Validation des adresses (lettre), des flags IMAP (garde-fou sur les
  caractères dangereux) et des MIME types des pièces jointes.
- Les pièces jointes sont transmises en base64 via l'outil.

## Utilisation (outil `email`)

```json
{"action": "list_folders"}
{"action": "list_messages", "mailbox": "INBOX", "limit": 20, "offset": 0}
{"action": "get_message", "mailbox": "INBOX", "uid": 42}
{"action": "search", "mailbox": "INBOX", "query": "UNSEEN", "limit": 10}
{"action": "send", "to": ["bob@example.com"], "subject": "Salut",
 "text_body": "Bonjour Bob"}
{"action": "send", "to": ["a@b.c"], "subject": "Doc",
 "html_body": "<p>Bonjour</p>",
 "attachments": [{"filename": "f.txt", "mime_type": "text/plain",
                  "bytes_base64": "SGVsbG8="}]}
{"action": "reply", "mailbox": "INBOX", "uid": 42, "body": "Merci !"}
{"action": "forward", "mailbox": "INBOX", "uid": 42, "to": ["c@d.e"]}
{"action": "mark_read", "mailbox": "INBOX", "uid": 42, "read": true}
{"action": "move", "mailbox": "INBOX", "uid": 42, "target_mailbox": "Archive"}
{"action": "delete", "mailbox": "INBOX", "uid": 42, "expunge": false}
```

## Tests

```bash
cargo test --lib email::
```

16 tests : parsing MIME (4), construction SMTP (7), permissions et audit (5).
Les tests ne contactent aucun serveur réel.
