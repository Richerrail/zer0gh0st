//! Chargement dynamique des modules fun (.so) avec vérification de signature.

use anyhow::{anyhow, Result};
use libloading::{Library, Symbol};
use sha2::Digest;
use std::ffi::c_void;
use std::path::{Path, PathBuf};

use zer0fun_api::{Buffer, Context, Event, Meta, Rect};

/// Module fun chargé dynamiquement.
pub struct LoadedModule {
    /// Bibliothèque dynamique.
    _lib: Library,

    /// Instance du module (pointeur opaque).
    instance: *mut c_void,

    /// Métadonnées du module.
    pub meta: Meta,

    /// Chemin du fichier .so.
    pub path: PathBuf,

    /// Fonctions exportées.
    fns: ModuleFns,
}

/// Fonctions exportées par un module fun.
struct ModuleFns {
    destroy: unsafe extern "C" fn(*mut c_void),
    on_event: unsafe extern "C" fn(*mut c_void, *const c_void, *mut c_void) -> bool,
    on_tick: unsafe extern "C" fn(*mut c_void, *mut c_void),
    draw: unsafe extern "C" fn(*const c_void, *const c_void, *mut c_void),
    on_mount: unsafe extern "C" fn(*mut c_void, *mut c_void),
    on_unmount: unsafe extern "C" fn(*mut c_void),
}

impl LoadedModule {
    /// Charge un module depuis un fichier .so.
    ///
    /// Vérifie la signature avant de charger.
    pub fn load(path: &Path, verify_signature: bool) -> Result<Self> {
        // Vérifier la signature si demandé
        if verify_signature {
            verify_module_signature(path)?;
        }

        // Charger la bibliothèque
        let lib = unsafe { Library::new(path)? };

        // Vérifier la version de l'API
        let api_version: Symbol<unsafe extern "C" fn() -> u32> =
            unsafe { lib.get(b"zer0fun_api_version")? };
        let version = unsafe { api_version() };
        if version != zer0fun_api::API_VERSION {
            return Err(anyhow!(
                "version API incompatible: module={}, attendu={}",
                version,
                zer0fun_api::API_VERSION
            ));
        }

        // Récupérer les métadonnées
        let meta_fn: Symbol<unsafe extern "C" fn() -> *mut std::ffi::c_char> =
            unsafe { lib.get(b"zer0fun_module_meta")? };
        let meta_ptr = unsafe { meta_fn() };
        let meta_json = unsafe { std::ffi::CString::from_raw(meta_ptr) };
        let meta: Meta = serde_json::from_str(meta_json.to_str()?)?;

        // Valider le nom
        meta.validate()?;

        // Créer le contexte initial
        let ctx = Box::new(Context::default());
        let ctx_ptr = Box::into_raw(ctx) as *mut c_void;

        // Créer l'instance
        let create_fn: Symbol<unsafe extern "C" fn(*mut c_void) -> *mut c_void> =
            unsafe { lib.get(b"zer0fun_module_create")? };
        let instance = unsafe { create_fn(ctx_ptr) };

        if instance.is_null() {
            return Err(anyhow!("échec de la création du module"));
        }

        // Récupérer les fonctions
        let fns = ModuleFns {
            destroy: unsafe { *lib.get(b"zer0fun_module_destroy")? },
            on_event: unsafe { *lib.get(b"zer0fun_module_on_event")? },
            on_tick: unsafe { *lib.get(b"zer0fun_module_on_tick")? },
            draw: unsafe { *lib.get(b"zer0fun_module_draw")? },
            on_mount: unsafe { *lib.get(b"zer0fun_module_on_mount")? },
            on_unmount: unsafe { *lib.get(b"zer0fun_module_on_unmount")? },
        };

        Ok(Self {
            _lib: lib,
            instance,
            meta,
            path: path.to_path_buf(),
            fns,
        })
    }

    /// Appelle on_mount sur le module.
    pub fn on_mount(&mut self, ctx: &mut Context) {
        let ctx_ptr = ctx as *mut Context as *mut c_void;
        unsafe {
            (self.fns.on_mount)(self.instance, ctx_ptr);
        }
    }

    /// Appelle on_unmount sur le module.
    pub fn on_unmount(&mut self) {
        unsafe {
            (self.fns.on_unmount)(self.instance);
        }
    }

    /// Traite un événement.
    pub fn on_event(&mut self, ev: &Event, ctx: &mut Context) -> bool {
        let ev_ptr = ev as *const Event as *const c_void;
        let ctx_ptr = ctx as *mut Context as *mut c_void;
        unsafe { (self.fns.on_event)(self.instance, ev_ptr, ctx_ptr) }
    }

    /// Appelle on_tick.
    pub fn on_tick(&mut self, ctx: &mut Context) {
        let ctx_ptr = ctx as *mut Context as *mut c_void;
        unsafe {
            (self.fns.on_tick)(self.instance, ctx_ptr);
        }
    }

    /// Dessine le module.
    pub fn draw(&self, area: &Rect, buf: &mut Buffer) {
        let area_ptr = area as *const Rect as *const c_void;
        let buf_ptr = buf as *mut Buffer as *mut c_void;
        unsafe {
            (self.fns.draw)(self.instance, area_ptr, buf_ptr);
        }
    }
}

impl Drop for LoadedModule {
    fn drop(&mut self) {
        if !self.instance.is_null() {
            unsafe {
                (self.fns.destroy)(self.instance);
            }
        }
    }
}

// Safety: le module est utilisé depuis un seul thread à la fois
unsafe impl Send for LoadedModule {}
unsafe impl Sync for LoadedModule {}

/// Vérifie la signature d'un module.
fn verify_module_signature(path: &Path) -> Result<()> {
    let sig_path = path.with_extension("so.sig");

    if !sig_path.exists() {
        return Err(anyhow!(
            "signature non trouvée: {}. Utiliser 'zer0build sign' pour signer.",
            sig_path.display()
        ));
    }

    // Charger la clé publique
    let pubkey_path = default_pubkey_path();
    if !pubkey_path.exists() {
        return Err(anyhow!(
            "clé publique non trouvée: {}. Utiliser 'zer0build sign' d'abord.",
            pubkey_path.display()
        ));
    }

    let pubkey_bytes = std::fs::read(&pubkey_path)?;
    let pubkey_bytes: [u8; 32] = pubkey_bytes
        .try_into()
        .map_err(|_| anyhow!("clé publique invalide"))?;
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&pubkey_bytes)
        .map_err(|e| anyhow!("clé publique invalide: {}", e))?;

    // Charger la signature
    let sig_bytes = std::fs::read(&sig_path)?;
    let signature = ed25519_dalek::Signature::from_bytes(
        &sig_bytes
            .try_into()
            .map_err(|_| anyhow!("signature invalide"))?,
    );

    // Calculer le hash du fichier
    let content = std::fs::read(path)?;
    let mut hasher = sha2::Sha256::new();
    sha2::Digest::update(&mut hasher, &content);
    let hash = hasher.finalize();

    // Vérifier
    ed25519_dalek::Verifier::verify(&verifying_key, &hash, &signature)
        .map_err(|_| anyhow!("signature invalide pour {}", path.display()))?;

    Ok(())
}

/// Chemin par défaut de la clé publique.
fn default_pubkey_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        dirs::config_dir()
            .unwrap_or_else(|| {
                PathBuf::from(
                    std::env::var("APPDATA")
                        .unwrap_or_else(|_| "C:\\Users\\Public\\AppData\\Roaming".to_string()),
                )
            })
            .join("zer0build")
            .join("verify_key.pub")
    }
    #[cfg(not(target_os = "windows"))]
    {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("~/.config"))
            .join("zer0build")
            .join("verify_key.pub")
    }
}
