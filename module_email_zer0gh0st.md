# Spécification — Module Email pour zer0gh0st

> **Fichier cible :** `docs/EMAIL_MODULE_SPEC.md`
> **Destinataire :** agent de code / contributeur Rust
> **Objectif :** ajouter au projet `Richerrail/zer0gh0st` un module email en Rust couvrant les
opérations **read**, **write** et **manage**, avec intégration propre dans l’architecture
existante.

---

## 0. Instructions pour l’agent

Tu es un agent de développement Rust travaillant sur le dépôt `zer0gh0st`.

Ta mission est d’implémenter un module email modulaire, testable et sécurisé, en respectant
les règles suivantes :

1. **Ne pas casser l’architecture existante.**
2. Réutiliser les conventions du projet pour les tools (`src/tools/`) et les mods (`src/fun/`).
3. Privilégier une implémentation **asynchrone** compatible Tokio.
4. Ne jamais stocker ni logger de secret en clair.
5. Livrer du code compilable avec `cargo check` et `cargo test`.
6. Découper l’implémentation en étapes vérifiables.
7. En cas d’ambiguïté, choisir l’option la plus simple, la plus sûre et la plus documentée.

**Livrable attendu :** une branche ou PR contenant le module email, ses tests, sa
documentation et son enregistrement comme outil appelable par l’agent.

---

## 1. Objectif fonctionnel

Le module doit permettre à zer0gh0st d’effectuer trois catégories d’actions :

### 1.1 Read — lecture

- Lister les dossiers IMAP.
- Lister les messages d’un dossier avec pagination.
- Récupérer un message par UID.
- Extraire les en-têtes, le corps texte, le corps HTML et les métadonnées des pièces jointes.
- Rechercher des messages par expéditeur, sujet, date ou mots-clés.

### 1.2 Write — écriture

- Envoyer un email simple.
- Envoyer un email HTML.
- Envoyer avec pièces jointes.
- Répondre à un message.
- Transférer un message.
- Créer un brouillon si le backend le permet.

### 1.3 Manage — gestion

- Marquer comme lu / non lu.
- Ajouter ou retirer un flag.
- Déplacer un message vers un dossier.
- Archiver un message.
- Supprimer un message.
- Vider la corbeille uniquement avec confirmation explicite.

---

## 2. Contraintes techniques

- **Langage :** Rust stable.
- **Runtime :** Tokio.
- **IMAP :** `async-imap`.
- **SMTP :** `lettre`.
- **Parsing MIME :** `mail-parser` ou `mailparse`.
- **Erreurs :** `thiserror`.
- **Sérialisation :** `serde` + `serde_json`.
- **Logs :** `tracing`, sans secrets.
- **Configuration :** variables d’environnement ou fichier local ignoré par Git.
- **Sécurité :** mots de passe d’application ou OAuth2, jamais de mot de passe principal.

---

## 3. Architecture proposée

Créer un module dédié :

```text
src/
  email/
   mod.rs
   config.rs
   error.rs
   types.rs
   imap_client.rs
   smtp_client.rs
   parser.rs
   service.rs
   tool.rs
  tools/
   email.rs       # adaptateur pour l’agent
  fun/
      mail.rs    # optionnel : mod TUI 0mail
```

### Rôle des fichiers

| Fichier | Responsabilité |
|---|---|
| `config.rs` | Chargement et validation de la configuration email. |
| `error.rs` | Erreurs typées du module. |
| `types.rs` | Structures `EmailSummary`, `EmailMessage`, `SendRequest`, etc. |
| `imap_client.rs` | Connexion IMAP, listing, fetch, search, flags, move, delete. |
| `smtp_client.rs` | Envoi SMTP via `lettre`. |
| `parser.rs` | Parsing MIME et extraction du corps/pièces jointes. |
| `service.rs` | Façade haut niveau `EmailService` utilisée par les tools et mods. |
| `tool.rs` | Définition des actions exposées à l’agent. |
| `src/tools/email.rs` | Enregistrement du tool dans le système existant. |
| `src/fun/mail.rs` | Optionnel : interface TUI `0mail`. |

---

## 4. Dépendances Cargo

Adapter les versions à celles déjà utilisées dans le projet.

```toml
[dependencies]
async-imap = "0.10"
lettre = { version = "0.11", features = ["tokio1-rustls-tls", "builder"] }
mail-parser = "0.9"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "time"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tracing = "0.1"
async-trait = "0.1"
```

Si le projet utilise déjà `rustls`, `native-tls`, `anyhow` ou `eyre`, rester cohérent avec
l’existant.

---

## 5. Configuration et secrets

### Variables d’environnement proposées

```text
ZER0_EMAIL_IMAP_HOST=imap.example.com
ZER0_EMAIL_IMAP_PORT=993
ZER0_EMAIL_IMAP_USER=user@example.com
ZER0_EMAIL_IMAP_PASSWORD=app-password
ZER0_EMAIL_SMTP_HOST=smtp.example.com
ZER0_EMAIL_SMTP_PORT=587
ZER0_EMAIL_SMTP_USER=user@example.com
ZER0_EMAIL_SMTP_PASSWORD=app-password
ZER0_EMAIL_FROM_NAME=Zer0
ZER0_EMAIL_FROM_ADDRESS=user@example.com
```

### Règles

- Ne jamais committer de credentials.
- Ne jamais afficher les mots de passe dans les logs ou les erreurs.
- Supporter les mots de passe d’application.
- Prévoir une extension OAuth2/XOAUTH2 sans l’implémenter dans la première itération.
- Valider la configuration au démarrage du service.
- Fournir un message d’erreur clair si la configuration est incomplète.

---

## 6. Types principaux

```rust
pub struct EmailConfig {
   pub imap_host: String,
   pub imap_port: u16,
   pub imap_user: String,
   pub imap_password: String,
   pub smtp_host: String,
   pub smtp_port: u16,
   pub smtp_user: String,
   pub smtp_password: String,
   pub from_name: Option<String>,
   pub from_address: String,
}

pub struct EmailSummary {
  pub uid: u32,
  pub mailbox: String,
  pub subject: Option<String>,
  pub from: Option<String>,
  pub to: Vec<String>,
  pub date: Option<String>,
  pub flags: Vec<String>,
  pub snippet: Option<String>,
}
pub struct EmailMessage {
  pub summary: EmailSummary,
  pub headers: Vec<(String, String)>,
  pub text_body: Option<String>,
  pub html_body: Option<String>,
  pub attachments: Vec<AttachmentMeta>,
}

pub struct AttachmentMeta {
  pub filename: Option<String>,
  pub mime_type: Option<String>,
  pub size: Option<usize>,
}

pub struct SendRequest {
  pub to: Vec<String>,
  pub cc: Vec<String>,
  pub bcc: Vec<String>,
  pub subject: String,
  pub text_body: Option<String>,
  pub html_body: Option<String>,
  pub attachments: Vec<AttachmentInput>,
}

pub struct AttachmentInput {
    pub filename: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}
```

---

## 7. Façade de service

```rust
#[async_trait::async_trait]
pub trait EmailServiceTrait {
   async fn list_folders(&self) -> Result<Vec<String>, EmailError>;
   async fn list_messages(&self, mailbox: &str, limit: u32, offset: u32) ->
Result<Vec<EmailSummary>, EmailError>;
   async fn get_message(&self, mailbox: &str, uid: u32) -> Result<EmailMessage,
EmailError>;
   async fn search(&self, mailbox: &str, query: &str, limit: u32) ->
Result<Vec<EmailSummary>, EmailError>;
   async fn send(&self, request: SendRequest) -> Result<(), EmailError>;
   async fn reply(&self, mailbox: &str, uid: u32, body: String) -> Result<(), EmailError>;
    async fn forward(&self, mailbox: &str, uid: u32, to: Vec<String>, body: Option<String>) ->
Result<(), EmailError>;
    async fn set_flags(&self, mailbox: &str, uid: u32, flags: Vec<String>, add: bool) ->
Result<(), EmailError>;
    async fn move_message(&self, mailbox: &str, uid: u32, target_mailbox: &str) -> Result<(),
EmailError>;
    async fn delete_message(&self, mailbox: &str, uid: u32, expunge: bool) -> Result<(),
EmailError>;
}
```

---

## 8. Interface exposée à l’agent

Créer une action sérialisable :

```rust
#[derive(Debug, serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum EmailAction {
    ListFolders,
    ListMessages { mailbox: String, limit: Option<u32>, offset: Option<u32> },
    GetMessage { mailbox: String, uid: u32 },
    Search { mailbox: String, query: String, limit: Option<u32> },
    Send { request: SendRequest },
    Reply { mailbox: String, uid: u32, body: String },
    Forward { mailbox: String, uid: u32, to: Vec<String>, body: Option<String> },
    MarkRead { mailbox: String, uid: u32, read: bool },
    Move { mailbox: String, uid: u32, target_mailbox: String },
    Delete { mailbox: String, uid: u32, expunge: bool },
}
```

### Exemple d’appel JSON

```json
{
  "action": "list_messages",
  "mailbox": "INBOX",
  "limit": 10,
  "offset": 0
}
```

```json
{
  "action": "send",
 "request": {
   "to": ["team@example.com"],
   "cc": [],
   "bcc": [],
   "subject": "Résumé",
   "text_body": "Voici le résumé demandé.",
   "html_body": null,
   "attachments": []
 }
}
```

---

## 9. Implémentation IMAP

### Lecture

- Se connecter en TLS sur le port IMAP.
- Authentifier avec `LOGIN`.
- Sélectionner le dossier demandé.
- Utiliser les UID pour identifier les messages.
- Récupérer les en-têtes et un extrait du corps.
- Parser le MIME avec `mail-parser`.
- Ne pas charger les pièces jointes complètes par défaut : retourner seulement leurs
métadonnées.

### Gestion

- `STORE` pour les flags `\Seen`, `\Flagged`, etc.
- `COPY` + `STORE \Deleted` + `EXPUNGE` pour déplacer ou supprimer selon le backend.
- Toujours protéger les opérations destructives par un paramètre explicite.
- Ne jamais faire d’`EXPUNGE` global implicite.

### Robustesse

- Gérer les erreurs de connexion, timeout et authentification.
- Prévoir une reconnexion simple ou retourner une erreur claire.
- Ne pas bloquer la boucle principale de l’agent.

---

## 10. Implémentation SMTP

- Construire le message avec `lettre::Message`.
- Supporter le texte brut et le HTML.
- Supporter les pièces jointes.
- Utiliser STARTTLS ou TLS implicite selon le port.
- Valider les adresses email avant envoi.
- Retourner une erreur typée en cas d’échec d’authentification ou de relais.

---

## 11. Sécurité et garde-fous

### Règles obligatoires

1. **Aucun secret en clair** dans le code, les logs ou les erreurs.
2. **Aucune suppression définitive** sans `expunge: true` explicite.
3. **Aucun envoi massif** sans limitation.
4. **Rate limiting** simple pour l’envoi.
5. **Audit log** des actions write/manage : action, mailbox, UID, timestamp, sans contenu
sensible.
6. **Séparation des permissions** :
  - `email.read`
  - `email.write`
  - `email.manage`
7. Les actions `delete` et `send` doivent pouvoir être refusées par configuration.

### Configuration de permissions proposée

```text
ZER0_EMAIL_ALLOW_READ=true
ZER0_EMAIL_ALLOW_WRITE=false
ZER0_EMAIL_ALLOW_MANAGE=false
ZER0_EMAIL_ALLOW_DELETE=false
```

---

## 12. Plan de travail pour l’agent

### Étape 1 — Scaffolding

- Créer `src/email/`.
- Ajouter les modules et les dépendances.
- Ajouter `EmailConfig`, `EmailError`, les types de base.
- Vérifier avec `cargo check`.

### Étape 2 — Lecture IMAP

- Implémenter `list_folders`.
- Implémenter `list_messages`.
- Implémenter `get_message`.
- Ajouter le parsing MIME.
- Ajouter des tests unitaires sur le parser avec des emails statiques.
### Étape 3 — Envoi SMTP

- Implémenter `send`.
- Ajouter le support texte, HTML et pièces jointes.
- Ajouter la validation des adresses.
- Ajouter un test de construction de message sans envoi réel.

### Étape 4 — Gestion

- Implémenter `mark_read`.
- Implémenter `move_message`.
- Implémenter `delete_message` avec garde-fou `expunge`.
- Journaliser les actions manage.

### Étape 5 — Intégration tool

- Créer `src/tools/email.rs`.
- Enregistrer le tool dans le registre existant.
- Exposer `EmailAction`.
- Vérifier que l’agent peut appeler les actions via JSON.

### Étape 6 — Tests et documentation

- Tests unitaires : parser, validation, config.
- Tests d’intégration avec traits mockés pour IMAP/SMTP.
- Documentation dans `docs/EMAIL.md`.
- Exemples d’utilisation.

### Étape 7 — Option TUI

- Créer `src/fun/mail.rs`.
- Afficher la boîte de réception.
- Navigation clavier simple.
- Ne pas bloquer l’agent principal.

---

## 13. Critères d’acceptation

La tâche est terminée lorsque :

- [ ] `cargo check` passe.
- [ ] `cargo test` passe.
- [ ] Le module email est isolé dans `src/email/`.
- [ ] Les actions `read`, `write` et `manage` sont exposées.
- [ ] Les secrets ne sont jamais loggés.
- [ ] Les actions destructives nécessitent un flag explicite.
- [ ] Le tool est enregistré et appelable.
- [ ] La documentation `docs/EMAIL.md` existe.
- [ ] Les permissions `read`, `write`, `manage` sont configurables.
- [ ] Un exemple JSON d’appel est fourni.

---

## 14. Tests recommandés

### Tests unitaires

- Chargement de configuration valide/invalide.
- Parsing d’un email texte simple.
- Parsing d’un email HTML.
- Parsing d’un email avec pièce jointe.
- Validation des adresses email.
- Construction d’un `SendRequest`.

### Tests d’intégration

- Utiliser des traits mockés pour `ImapClient` et `SmtpClient`.
- Vérifier que `delete_message` sans `expunge` ne supprime pas définitivement.
- Vérifier que les permissions bloquent les actions non autorisées.
- Vérifier que les logs ne contiennent pas de mot de passe.

---

## 15. Ce qu’il ne faut pas faire

- Ne pas hardcoder d’identifiants.
- Ne pas utiliser de mot de passe principal de compte email.
- Ne pas exposer les emails complets dans les logs.
- Ne pas faire d’`EXPUNGE` global.
- Ne pas bloquer le runtime async avec des appels synchrones longs.
- Ne pas mélanger la logique IMAP/SMTP avec la logique TUI.
- Ne pas ajouter de dépendance lourde sans justification.

---

## 16. Résultat attendu

À la fin, zer0gh0st doit pouvoir exécuter des actions comme :

```text
email.list_folders
email.list_messages { mailbox: "INBOX", limit: 5 }
email.get_message { mailbox: "INBOX", uid: 42 }
email.search { mailbox: "INBOX", query: "facture" }
email.send { to: ["team@example.com"], subject: "Rapport", text_body: "..." }
email.mark_read { mailbox: "INBOX", uid: 42, read: true }
email.move { mailbox: "INBOX", uid: 42, target_mailbox: "Archive" }
email.delete { mailbox: "Trash", uid: 42, expunge: false }
```

Le tout en Rust, de manière asynchrone, sécurisée et intégrée à l’architecture existante de
zer0gh0st.

