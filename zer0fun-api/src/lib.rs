//! # zer0fun-api
//!
//! API stable pour les modules fun de zer0gh0st.
//!
//! Un module fun est une bibliothèque dynamique (.so/.dll/.dylib) qui implémente
//! le trait [`FunModule`] et s'enregistre avec la macro [`register!`].
//!
//! ## Exemple minimal
//!
//! ```rust
//! use zer0fun_api::{FunModule, Context, Event, Meta, Rect, Buffer, KeyEvent};
//!
//! pub struct Compteur { n: u32 }
//!
//! impl FunModule for Compteur {
//!     fn meta() -> Meta {
//!         Meta::new("compteur", "0.1.0", "Compteur simple")
//!     }
//!
//!     fn new(_ctx: Context) -> Self {
//!         Self { n: 0 }
//!     }
//!
//!     fn on_event(&mut self, ev: Event, ctx: &mut Context) -> bool {
//!         match ev {
//!             Event::Key(KeyEvent::Char('+')) => { self.n += 1; ctx.redraw = true; true }
//!             Event::Key(KeyEvent::Char('-')) => { self.n -= 1; ctx.redraw = true; true }
//!             Event::Key(KeyEvent::Esc) => { ctx.exit = true; true }
//!             _ => false,
//!         }
//!     }
//!
//!     fn draw(&self, area: Rect, buf: &mut Buffer) {
//!         let text = format!("Compteur: {}", self.n);
//!         buf.set_string(area.x, area.y, &text);
//!     }
//! }
//!
//! zer0fun_api::register!(Compteur);
//! ```

use serde::{Deserialize, Serialize};

pub mod buffer;
pub mod context;
pub mod event;
pub mod meta;
pub mod rect;

pub use buffer::{BorderStyle, Buffer, Cell, Color, Style};
pub use context::Context;
pub use event::{Event, KeyEvent, MouseEvent};
pub use meta::{Category, Meta, Permissions};
pub use rect::Rect;

/// Erreurs de l'API zer0fun
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("erreur d'initialisation: {0}")]
    Init(String),

    #[error("erreur de rendu: {0}")]
    Draw(String),

    #[error("erreur d'événement: {0}")]
    Event(String),

    #[error("erreur de configuration: {0}")]
    Config(String),

    #[error("erreur réseau: {0}")]
    Network(String),

    #[error("erreur fichier: {0}")]
    Io(#[from] std::io::Error),

    #[error("erreur inconnue: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Trait principal que tout module fun doit implémenter.
///
/// # Cycle de vie
///
/// 1. `meta()` — appelé une fois pour récupérer les métadonnées
/// 2. `new(ctx)` — création de l'instance
/// 3. `on_mount(ctx)` — le module devient visible
/// 4. Boucle: `on_event()` / `on_tick()` / `draw()`
/// 5. `on_unmount()` — le module est caché
/// 6. Drop — destruction
///
/// # Thread-safety
///
/// Les modules doivent être `Send + Sync`. zer0gh0st peut appeler
/// les méthodes depuis différents threads (UI, réseau, etc.).
pub trait FunModule: Send + Sync + 'static {
    /// Métadonnées du module (nom, version, auteur, etc.)
    fn meta() -> Meta
    where
        Self: Sized;

    /// Crée une nouvelle instance du module.
    ///
    /// Le [`Context`] contient les arguments de la ligne de commande,
    /// le répertoire de données persistantes, et la configuration.
    fn new(ctx: Context) -> Self
    where
        Self: Sized;

    /// Appelé quand le module devient visible (monté).
    ///
    /// Utile pour démarrer des timers, des connexions réseau, etc.
    fn on_mount(&mut self, _ctx: &mut Context) {}

    /// Appelé quand le module est caché (démonté).
    ///
    /// Utile pour sauvegarder l'état, fermer des connexions, etc.
    fn on_unmount(&mut self) {}

    /// Traite un événement utilisateur.
    ///
    /// Retourne `true` si l'événement a été consommé (ne sera pas
    /// propagé aux autres modules ou à zer0gh0st).
    fn on_event(&mut self, ev: Event, ctx: &mut Context) -> bool;

    /// Appelé à intervalle régulier (par défaut ~60 FPS).
    ///
    /// Utiliser pour les animations, les mises à jour périodiques.
    /// Ne pas bloquer : les opérations longues doivent être async.
    fn on_tick(&mut self, _ctx: &mut Context) {}

    /// Dessine le module dans la zone donnée.
    ///
    /// Le [`Buffer`] est une abstraction au-dessus de ratatui.
    /// Pour des helpers ratatui avancés, activer la feature `ratatui-helpers`.
    fn draw(&self, area: Rect, buf: &mut Buffer);

    /// Appelé quand la configuration du module change.
    fn on_config_change(&mut self, _ctx: &mut Context) {}
}

/// Configuration d'un module (fournie par zer0gh0st).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleConfig {
    /// Valeurs de configuration spécifiques au module
    #[serde(flatten)]
    pub values: serde_json::Value,

    /// Thème actif de zer0gh0st
    pub theme: String,

    /// Langue préférée (ex: "fr", "en")
    pub lang: String,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self {
            values: serde_json::Value::Object(serde_json::Map::new()),
            theme: "default".to_string(),
            lang: "fr".to_string(),
        }
    }
}

/// Macro pour enregistrer un module fun.
///
/// Génère les fonctions C exportées que zer0gh0st charge dynamiquement.
///
/// # Exemple
///
/// ```rust
/// zer0fun_api::register!(MonModule);
/// ```
#[macro_export]
macro_rules! register {
    ($module:ty) => {
        /// Fonction d'entrée principale appelée par zer0gh0st.
        ///
        /// # Safety
        ///
        /// Cette fonction est appelée depuis zer0gh0st via libloading.
        /// Elle doit être `extern "C"` et ne pas paniquer.
        #[no_mangle]
        pub extern "C" fn zer0fun_module_create(
            ctx: *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void {
            let ctx = unsafe {
                let ctx = ctx as *mut $crate::Context;
                *Box::from_raw(ctx)
            };
            let module = <$module as $crate::FunModule>::new(ctx);
            Box::into_raw(Box::new(module)) as *mut std::ffi::c_void
        }

        /// Détruit une instance de module.
        ///
        /// # Safety
        ///
        /// Le pointeur doit provenir de `zer0fun_module_create`.
        #[no_mangle]
        pub unsafe extern "C" fn zer0fun_module_destroy(ptr: *mut std::ffi::c_void) {
            if !ptr.is_null() {
                let _ = Box::from_raw(ptr as *mut $module);
            }
        }

        /// Retourne les métadonnées du module.
        ///
        /// # Safety
        ///
        /// Le pointeur retourné doit être libéré avec `zer0fun_meta_free`.
        #[no_mangle]
        pub extern "C" fn zer0fun_module_meta() -> *mut std::ffi::c_char {
            let meta = <$module as $crate::FunModule>::meta();
            let json = serde_json::to_string(&meta).unwrap_or_default();
            std::ffi::CString::new(json).unwrap().into_raw()
        }

        /// Libère une chaîne de métadonnées.
        ///
        /// # Safety
        ///
        /// Le pointeur doit provenir de `zer0fun_module_meta`.
        #[no_mangle]
        pub unsafe extern "C" fn zer0fun_meta_free(ptr: *mut std::ffi::c_char) {
            if !ptr.is_null() {
                let _ = std::ffi::CString::from_raw(ptr);
            }
        }

        /// Traite un événement.
        ///
        /// # Safety
        ///
        /// Les pointeurs doivent être valides.
        #[no_mangle]
        pub unsafe extern "C" fn zer0fun_module_on_event(
            module: *mut std::ffi::c_void,
            ev: *const std::ffi::c_void,
            ctx: *mut std::ffi::c_void,
        ) -> bool {
            let module = &mut *(module as *mut $module);
            let ev = &*(ev as *const $crate::Event);
            let ctx = &mut *(ctx as *mut $crate::Context);
            module.on_event(ev.clone(), ctx)
        }

        /// Appelé à chaque tick.
        ///
        /// # Safety
        ///
        /// Les pointeurs doivent être valides.
        #[no_mangle]
        pub unsafe extern "C" fn zer0fun_module_on_tick(
            module: *mut std::ffi::c_void,
            ctx: *mut std::ffi::c_void,
        ) {
            let module = &mut *(module as *mut $module);
            let ctx = &mut *(ctx as *mut $crate::Context);
            module.on_tick(ctx);
        }

        /// Dessine le module.
        ///
        /// # Safety
        ///
        /// Les pointeurs doivent être valides.
        #[no_mangle]
        pub unsafe extern "C" fn zer0fun_module_draw(
            module: *mut std::ffi::c_void,
            area: *const std::ffi::c_void,
            buf: *mut std::ffi::c_void,
        ) {
            let module = &mut *(module as *mut $module);
            let area = &*(area as *const $crate::Rect);
            let buf = &mut *(buf as *mut $crate::Buffer);
            module.draw(*area, buf);
        }

        /// Appelé quand le module est monté.
        #[no_mangle]
        pub unsafe extern "C" fn zer0fun_module_on_mount(
            module: *mut std::ffi::c_void,
            ctx: *mut std::ffi::c_void,
        ) {
            let module = &mut *(module as *mut $module);
            let ctx = &mut *(ctx as *mut $crate::Context);
            module.on_mount(ctx);
        }

        /// Appelé quand le module est démonté.
        #[no_mangle]
        pub unsafe extern "C" fn zer0fun_module_on_unmount(module: *mut std::ffi::c_void) {
            let module = &mut *(module as *mut $module);
            module.on_unmount();
        }
    };
}

/// Version de l'API (pour vérification de compatibilité).
pub const API_VERSION: u32 = 1;

/// Vérifie la compatibilité de version.
#[no_mangle]
pub extern "C" fn zer0fun_api_version() -> u32 {
    API_VERSION
}
