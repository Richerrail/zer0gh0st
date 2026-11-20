//! Client IMAP (read / manage).
//! Connexion TLS, login, list, fetch, search, store, copy, expunge.
//! Aucun secret n'est loggué.

use crate::email::config::EmailConfig;
use crate::email::error::EmailError;
use crate::email::parser::parse_email;
use crate::email::types::{EmailMessage, EmailSummary};
use async_imap::types::Flag;
use async_imap::Session;
use futures_util::StreamExt;
use tokio::net::TcpStream;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;
use tracing::debug;

type TlsStream = tokio_rustls::client::TlsStream<TcpStream>;

/// Client IMAP avec session active.
pub struct ImapClient {
    session: Session<TlsStream>,
}

impl ImapClient {
    /// Crée une nouvelle connexion IMAP TLS et s'authentifie.
    pub async fn new(config: &EmailConfig) -> Result<Self, EmailError> {
        let addr = format!("{}:{}", config.imap_host, config.imap_port);
        debug!("Connexion IMAP à {}", addr);

        let tcp = TcpStream::connect(&addr)
            .await
            .map_err(|e| EmailError::Imap(format!("Connexion TCP échouée: {}", e)))?;

        let mut root_store = RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().map(|a| a.to_owned()));

        let client_config = ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();

        let connector = TlsConnector::from(std::sync::Arc::new(client_config));
        let domain = rustls::pki_types::ServerName::try_from(config.imap_host.as_str())
            .map_err(|_| EmailError::Imap("Nom de serveur invalide".to_string()))?
            .to_owned();

        let tls = connector
            .connect(domain, tcp)
            .await
            .map_err(|e| EmailError::Imap(format!("Handshake TLS échoué: {}", e)))?;

        let client = async_imap::Client::new(tls);
        let session = client
            .login(&config.imap_user, &config.imap_password)
            .await
            .map_err(|(e, _)| EmailError::Imap(format!("Login échoué: {}", e)))?;

        debug!("Connecté et authentifié");
        Ok(Self { session })
    }

    /// Liste les dossiers IMAP.
    pub async fn list_folders(&mut self) -> Result<Vec<String>, EmailError> {
        let mut names = self
            .session
            .list(Some(""), Some("*"))
            .await
            .map_err(|e| EmailError::Imap(format!("LIST échoué: {}", e)))?;

        let mut folders: Vec<String> = Vec::new();
        while let Some(name) = names
            .next()
            .await
            .transpose()
            .map_err(|e| EmailError::Imap(format!("LIST stream échoué: {}", e)))?
        {
            if !name
                .attributes()
                .iter()
                .any(|a| matches!(a, async_imap::types::NameAttribute::NoSelect))
            {
                folders.push(name.name().to_string());
            }
        }

        debug!("Dossiers trouvés: {}", folders.len());
        Ok(folders)
    }

    /// Liste les messages d'un dossier avec pagination.
    pub async fn list_messages(
        &mut self,
        mailbox: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<EmailSummary>, EmailError> {
        let mailbox_info = self
            .session
            .select(mailbox)
            .await
            .map_err(|e| EmailError::Imap(format!("SELECT échoué: {}", e)))?;

        let total = mailbox_info.exists;
        if total == 0 {
            return Ok(Vec::new());
        }

        let end = total.saturating_sub(offset);
        let start = end.saturating_sub(limit).max(1);

        self.fetch_range(mailbox, start, end).await
    }

    /// Récupère un message par UID.
    pub async fn get_message(
        &mut self,
        mailbox: &str,
        uid: u32,
    ) -> Result<EmailMessage, EmailError> {
        self.session
            .select(mailbox)
            .await
            .map_err(|e| EmailError::Imap(format!("SELECT échoué: {}", e)))?;

        let mut fetch = self
            .session
            .uid_fetch(uid.to_string(), "BODY[]")
            .await
            .map_err(|e| EmailError::Imap(format!("UID FETCH échoué: {}", e)))?;

        let msg = fetch
            .next()
            .await
            .ok_or_else(|| EmailError::Imap("Message non trouvé".to_string()))?
            .map_err(|e| EmailError::Imap(format!("Fetch error: {}", e)))?;

        let body = msg
            .body()
            .ok_or_else(|| EmailError::Imap("Corps du message manquant".to_string()))?;

        let mut email = parse_email(body)?;
        email.summary.uid = uid;
        email.summary.mailbox = mailbox.to_string();
        Ok(email)
    }

    /// Recherche des messages.
    pub async fn search(
        &mut self,
        mailbox: &str,
        query: &str,
        limit: u32,
    ) -> Result<Vec<EmailSummary>, EmailError> {
        self.session
            .select(mailbox)
            .await
            .map_err(|e| EmailError::Imap(format!("SELECT échoué: {}", e)))?;

        let uids = self
            .session
            .uid_search(query)
            .await
            .map_err(|e| EmailError::Imap(format!("UID SEARCH échoué: {}", e)))?;

        let mut results = Vec::new();
        for uid in uids.into_iter().take(limit as usize) {
            if let Ok(msg) = self.get_message(mailbox, uid).await {
                results.push(msg.summary);
            }
        }

        debug!("Recherche '{}' : {} résultats", query, results.len());
        Ok(results)
    }

    /// Ajoute ou retire des flags.
    pub async fn set_flags(
        &mut self,
        mailbox: &str,
        uid: u32,
        flags: Vec<String>,
        add: bool,
    ) -> Result<(), EmailError> {
        self.session
            .select(mailbox)
            .await
            .map_err(|e| EmailError::Imap(format!("SELECT échoué: {}", e)))?;

        let action = if add { "+FLAGS" } else { "-FLAGS" };
        // Validation basique : chaque flag doit être un token IMAP sûr.
        for f in &flags {
            if f.is_empty() || f.contains(' ') || f.contains('(') || f.contains(')') {
                return Err(EmailError::Guardrail(format!("Flag invalide: {}", f)));
            }
        }
        let query = format!("{} ({})", action, flags.join(" "));

        let mut store_stream = self
            .session
            .uid_store(uid.to_string(), query)
            .await
            .map_err(|e| EmailError::Imap(format!("UID STORE échoué: {}", e)))?;
        // Drain le stream pour consommer la réponse du serveur.
        while let Some(item) = store_stream.next().await {
            item.map_err(|e| EmailError::Imap(format!("STORE stream échoué: {}", e)))?;
        }

        debug!("Flags {} pour UID {} dans {}", action, uid, mailbox);
        Ok(())
    }

    /// Déplace un message vers un autre dossier.
    pub async fn move_message(
        &mut self,
        mailbox: &str,
        uid: u32,
        target_mailbox: &str,
    ) -> Result<(), EmailError> {
        self.session
            .select(mailbox)
            .await
            .map_err(|e| EmailError::Imap(format!("SELECT échoué: {}", e)))?;

        self.session
            .uid_mv(uid.to_string(), target_mailbox)
            .await
            .map_err(|e| EmailError::Imap(format!("UID MOVE échoué: {}", e)))?;

        debug!(
            "Message {} déplacé de {} vers {}",
            uid, mailbox, target_mailbox
        );
        Ok(())
    }

    /// Supprime un message (avec garde-fou expunge).
    pub async fn delete_message(
        &mut self,
        mailbox: &str,
        uid: u32,
        expunge: bool,
    ) -> Result<(), EmailError> {
        self.session
            .select(mailbox)
            .await
            .map_err(|e| EmailError::Imap(format!("SELECT échoué: {}", e)))?;

        {
            let mut del_stream = self
                .session
                .uid_store(uid.to_string(), "+FLAGS (\\Deleted)")
                .await
                .map_err(|e| EmailError::Imap(format!("UID STORE échoué: {}", e)))?;
            // Drain le stream pour consommer la réponse du serveur.
            while let Some(item) = del_stream.next().await {
                item.map_err(|e| EmailError::Imap(format!("STORE stream échoué: {}", e)))?;
            }
        } // del_stream droppé ici, libère l'emprunt de self.session

        if expunge {
            let mut exp_stream = std::pin::pin!(self
                .session
                .expunge()
                .await
                .map_err(|e| EmailError::Imap(format!("EXPUNGE échoué: {}", e)))?);
            // Drain le stream pour consommer la réponse du serveur.
            while let Some(item) = exp_stream.next().await {
                item.map_err(|e| EmailError::Imap(format!("EXPUNGE stream échoué: {}", e)))?;
            }
            debug!("Message {} supprimé définitivement de {}", uid, mailbox);
        } else {
            debug!("Message {} marqué comme supprimé dans {}", uid, mailbox);
        }

        Ok(())
    }

    /// Déconnexion propre.
    pub async fn logout(&mut self) -> Result<(), EmailError> {
        self.session
            .logout()
            .await
            .map_err(|e| EmailError::Imap(format!("LOGOUT échoué: {}", e)))
    }

    /// Fetch d'une plage de messages (helper interne).
    async fn fetch_range(
        &mut self,
        mailbox: &str,
        start: u32,
        end: u32,
    ) -> Result<Vec<EmailSummary>, EmailError> {
        // `start`/`end` sont des numéros de séquence (1..=total) : on utilise
        // FETCH (et non UID FETCH), sinon les UID réels (souvent ≫ total sur
        // Gmail) ne correspondent à rien et la liste revient vide.
        let range = format!("{}:{}", start, end);
        let mut fetch = self
            .session
            .fetch(range, "(UID FLAGS BODY.PEEK[HEADER])")
            .await
            .map_err(|e| EmailError::Imap(format!("FETCH échoué: {}", e)))?;

        let mut summaries = Vec::new();
        while let Some(item) = fetch.next().await {
            let msg = item.map_err(|e| EmailError::Imap(format!("Fetch error: {}", e)))?;
            let uid = msg.uid.unwrap_or(0);
            let flags: Vec<String> = msg
                .flags()
                .map(|f| match f {
                    Flag::Seen => "\\Seen".to_string(),
                    Flag::Answered => "\\Answered".to_string(),
                    Flag::Flagged => "\\Flagged".to_string(),
                    Flag::Deleted => "\\Deleted".to_string(),
                    Flag::Draft => "\\Draft".to_string(),
                    Flag::Recent => "\\Recent".to_string(),
                    Flag::MayCreate => "\\*".to_string(),
                    Flag::Custom(c) => c.to_string(),
                })
                .collect();

            let mut summary = EmailSummary {
                uid,
                mailbox: mailbox.to_string(),
                subject: None,
                from: None,
                to: Vec::new(),
                date: None,
                flags,
                snippet: None,
            };

            if let Some(header) = msg.header() {
                if let Ok(parsed) = parse_email(header) {
                    summary.subject = parsed.summary.subject;
                    summary.from = parsed.summary.from;
                    summary.to = parsed.summary.to;
                    summary.date = parsed.summary.date;
                    summary.snippet = parsed.summary.snippet;
                }
            }

            summaries.push(summary);
        }

        Ok(summaries)
    }
}
