//! Registry dynamique des modules fun avec hot-reload.

use anyhow::{anyhow, Result};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

use zer0fun_api::{Context, Event, Rect, Buffer};

use super::loader::LoadedModule;

/// Registry des modules fun.
pub struct FunRegistry {
    /// Modules chargés (nom → module).
    modules: HashMap<String, LoadedModule>,
    
    /// Répertoire des modules.
    pub modules_dir: PathBuf,
    
    /// Watcher pour le hot-reload.
    _watcher: RecommendedWatcher,
    
    /// Récepteur d'événements de fichiers.
    watcher_rx: Receiver<notify::Result<notify::Event>>,
    
    /// Dernière vérification des modifications.
    last_check: Instant,
    
    /// Vérifier les signatures des modules.
    verify_signatures: bool,
}

impl FunRegistry {
    /// Crée un nouveau registry.
    pub fn new(modules_dir: Option<PathBuf>, verify_signatures: bool) -> Result<Self> {
        let modules_dir = modules_dir.unwrap_or_else(default_modules_dir);
        std::fs::create_dir_all(&modules_dir)?;
        
        // Créer le watcher
        let (tx, rx) = channel();
        let mut watcher = notify::recommended_watcher(tx)?;
        watcher.watch(&modules_dir, RecursiveMode::NonRecursive)?;
        
        let mut registry = Self {
            modules: HashMap::new(),
            modules_dir,
            _watcher: watcher,
            watcher_rx: rx,
            last_check: Instant::now(),
            verify_signatures,
        };
        
        // Charger les modules existants
        registry.load_all()?;
        
        Ok(registry)
    }
    
    /// Charge tous les modules du répertoire.
    pub fn load_all(&mut self) -> Result<()> {
        let entries = std::fs::read_dir(&self.modules_dir)?;
        
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            
            // Ignorer les non-.so
            if path.extension().map(|e| e != "so").unwrap_or(true) {
                continue;
            }
            
            // Ignorer les signatures
            if path.extension().map(|e| e == "sig").unwrap_or(false) {
                continue;
            }
            
            match self.load_module(&path) {
                Ok(name) => {
                    log::info!("Module '{}' chargé", name);
                }
                Err(e) => {
                    log::warn!("Échec du chargement de {}: {}", path.display(), e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Charge un module depuis un fichier.
    fn load_module(&mut self, path: &Path) -> Result<String> {
        let module = LoadedModule::load(path, self.verify_signatures)?;
        let name = module.meta.name.clone();
        
        // Décharger l'ancien si présent
        if let Some(mut old) = self.modules.remove(&name) {
            old.on_unmount();
        }
        
        self.modules.insert(name.clone(), module);
        Ok(name)
    }
    
    /// Décharge un module.
    pub fn unload_module(&mut self, name: &str) -> Result<()> {
        if let Some(mut module) = self.modules.remove(name) {
            module.on_unmount();
            log::info!("Module '{}' déchargé", name);
            Ok(())
        } else {
            Err(anyhow!("module '{}' non trouvé", name))
        }
    }
    
    /// Recharge un module.
    pub fn reload_module(&mut self, name: &str) -> Result<()> {
        let path = self.modules_dir.join(format!("{}.so", name));
        
        if !path.exists() {
            return Err(anyhow!("module '{}' non trouvé: {}", name, path.display()));
        }
        
        self.load_module(&path)?;
        log::info!("Module '{}' rechargé", name);
        Ok(())
    }
    
    /// Vérifie les modifications de fichiers (hot-reload).
    pub fn check_updates(&mut self) -> Result<Vec<String>> {
        let mut reloaded = Vec::new();
        
        // Limiter la fréquence de vérification
        if self.last_check.elapsed() < Duration::from_millis(500) {
            return Ok(reloaded);
        }
        self.last_check = Instant::now();
        
        // Traiter les événements du watcher
        while let Ok(event) = self.watcher_rx.try_recv() {
            let event = match event {
                Ok(e) => e,
                Err(e) => {
                    log::warn!("Erreur watcher: {}", e);
                    continue;
                }
            };
            
            // Filtrer les événements de modification/création
            if !matches!(
                event.kind,
                notify::EventKind::Create(_) | notify::EventKind::Modify(_)
            ) {
                continue;
            }
            
            for path in event.paths {
                // Ignorer les non-.so
                if path.extension().map(|e| e != "so").unwrap_or(true) {
                    continue;
                }
                
                // Extraire le nom du module
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .trim_start_matches("lib")
                    .to_string();
                
                if name.is_empty() {
                    continue;
                }
                
                // Recharger le module
                match self.reload_module(&name) {
                    Ok(()) => {
                        reloaded.push(name.clone());
                    }
                    Err(e) => {
                        log::warn!("Échec du rechargement de '{}': {}", name, e);
                    }
                }
            }
        }
        
        Ok(reloaded)
    }
    
    /// Retourne un module par nom.
    pub fn get(&self, name: &str) -> Option<&LoadedModule> {
        self.modules.get(name)
    }
    
    /// Retourne un module mutable par nom.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut LoadedModule> {
        self.modules.get_mut(name)
    }
    
    /// Liste les noms des modules chargés.
    pub fn list(&self) -> Vec<&str> {
        self.modules.keys().map(|s| s.as_str()).collect()
    }
    
    /// Liste les métadonnées des modules chargés.
    pub fn list_meta(&self) -> Vec<&zer0fun_api::Meta> {
        self.modules.values().map(|m| &m.meta).collect()
    }
    
    /// Vérifie si un module est chargé.
    pub fn contains(&self, name: &str) -> bool {
        self.modules.contains_key(name)
    }
    
    /// Nombre de modules chargés.
    pub fn len(&self) -> usize {
        self.modules.len()
    }
    
    /// Vérifie si le registry est vide.
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
    
    /// Traite un événement pour un module.
    pub fn on_event(&mut self, name: &str, ev: &Event, ctx: &mut Context) -> bool {
        if let Some(module) = self.modules.get_mut(name) {
            module.on_event(ev, ctx)
        } else {
            false
        }
    }
    
    /// Appelle on_tick pour un module.
    pub fn on_tick(&mut self, name: &str, ctx: &mut Context) {
        if let Some(module) = self.modules.get_mut(name) {
            module.on_tick(ctx);
        }
    }
    
    /// Dessine un module.
    pub fn draw(&self, name: &str, area: &Rect, buf: &mut Buffer) {
        if let Some(module) = self.modules.get(name) {
            module.draw(area, buf);
        }
    }
    
    /// Appelle on_mount pour un module.
    pub fn on_mount(&mut self, name: &str, ctx: &mut Context) {
        if let Some(module) = self.modules.get_mut(name) {
            module.on_mount(ctx);
        }
    }
    
    /// Appelle on_unmount pour un module.
    pub fn on_unmount(&mut self, name: &str) {
        if let Some(module) = self.modules.get_mut(name) {
            module.on_unmount();
        }
    }
}

impl Drop for FunRegistry {
    fn drop(&mut self) {
        // Décharger tous les modules proprement
        for (_, mut module) in self.modules.drain() {
            module.on_unmount();
        }
    }
}

/// Répertoire par défaut des modules.
fn default_modules_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from(std::env::var("APPDATA").unwrap_or_else(|_| "C:\\Users\\Public\\AppData\\Roaming".to_string())))
            .join("zer0gh0st")
            .join("fun")
    }
    #[cfg(not(target_os = "windows"))]
    {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"))
            .join("zer0gh0st")
            .join("fun")
    }
}
