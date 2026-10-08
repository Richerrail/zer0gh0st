# Brainstorm — Intégrer « Zero-chan » dans ZER0v1

Analyse de `~/zerochan` (52 Mo, Python/PyQt6).

---

## 1. Inventaire de Zero-chan (ce qui existe)

| Composant | Fichier | Techno |
|---|---|---|
| **App principale** | `zerochan.py` (20 Ko) | PyQt6, hotkey, pipeline audio, GIFs |
| **Cerveau** | `zerobrain_nvidia.py` / `zerobrain_local.py` | `faster-whisper` (STT) + NVIDIA Nemotron (LLM) |
| **Bridge** | `zero_bridge.py` | FIFO `/tmp/zero_bridge`, messages UTF-8 \0-délimités |
| **Avatar** | `AvatarWidget` | 12 GIFs (`image.gif`…`image11.gif`), rotation 3 s, GIF par action 5 s, bulle chat, input, visualiseur, idle bounce |
| **Audio in** | `AudioCapture` | `QAudioSource` 16 kHz mono |
| **Audio out** | `AudioPlayer` | `pw-play` (PipeWire) |
| **Actions** | `actions/*.yaml` (web/system/dev/music) | plugins YAML : triggers → URL/commande + GIF + WAV |
| **Loader** | `plugins/loader.py` | `ActionRegistry` / `ActionPlugin` |
| **Voix** | `audio_zerochan/` (5,8 Mo) | WAV **pré-enregistrés** (Qwen3-TTS voice cloning), par catégorie |
| **Assets** | `image*.gif` | 12 avatars animés |
| **Hotkey** | xbindkeys → fichier trigger | X11 |

### Pipeline
```
Micro 16 kHz → Whisper (local) → texte
     → LLM (persona anime) → réponse + détection action/émotion/GIF
     → exécution action (webbrowser/subprocess)
     → WAV contextuel (pw-play) + GIF spécifique (3 s)
     → retour rotation aléatoire (5 s)
```

---

## 2. Ce que ZER0v1 apporte déjà (à réutiliser)

| Besoin de Zero-chan | Existant dans ZER0v1 |
|---|---|
| LLM conversationnel | ✅ client multi-provider + failover + rôles |
| Persona anime | ✅ personas (`/whoami`) |
| Mémoire/Contexte | ✅ `memory.rs`, `compact.rs`, `knowledge.rs` |
| Exécution d'actions | ✅ `code_exec`, tools, `mods.rs` (lanceurs) |
| Bridge IPC | ❌ à faire |
| STT / TTS / Avatar | ❌ à faire |

---

## 3. Les vraies difficultés (classées)

1. **Overlay GUI sur Wayland/Hyprland** ⚠️ *le plus dur*
   - Transparence + always-on-top + position : PyQt6 le fait via XWayland, pas en Wayland natif.
   - Wayland natif → `wlr-layer-shell` (non supporté par tous les toolkits).
   - Rust : `egui`/`eframe` (fenêtre transparente possible), `iced`, `tauri` (webview), `slint`.
2. **STT Whisper** — `whisper-rs` (whisper.cpp, Rust) ou service Python.
3. **TTS** — ici c'est **pré-enregistré** (juste jouer des WAV) → facile. Synthèse dynamique = dur.
4. **Hotkey globale** — xbindkeys = X11 ; Hyprland → `bind` natif ou daemon evdev.
5. **Lip-sync / visualiseur** — cosmétique, optionnel.
6. **Découplage cerveau/visage** — aujourd'hui couplés dans un seul process Python.

---

## 4. Options d'architecture

### A. Portage Rust **complet**
`egui`+`eframe` (overlay) · `cpal`/`rodio` (audio) · `whisper-rs` (STT) · `gif` (anim) ·
`serde_yaml` (actions) · socket Unix (IPC).
→ **Autonome**, mais **gros chantier**, et l'overlay Wayland reste un risque.

### B. Réutiliser le **service Python** existant
ZER0v1 pilote `zerochan.py` via FIFO/socket.
→ Rapide, mais **dépend de Python/PyQt6 + du dossier** (contredit l'autonomie).

### C. Hybride **recommandé** — deux binaires dans ZER0v1
- `zer0` (déjà) = **le cerveau** : LLM, persona, mémoire, connaissance, actions.
- `zer0chan` (nouveau) = **le visage + la voix** : avatar + audio + STT, connecté par **socket Unix JSON**.
→ Découplage propre, testable, et on peut faire `zer0chan` en Rust **progressivement**.

### D. Variante **100 % TUI** (la plus fidèle à ZER0v1)
Pas de GUI : un **panneau latéral animé** (comme `/bintime`, `/radio`) affichant l'état
(`idle` / `écoute` / `réfléchit` / `parle`) avec des **frames ASCII/emoji**, + WAV joués via
`ffplay`. → Évite **tout** le problème overlay Wayland, mais perd l'anime GIF.

---

## 5. Découpage en phases (proposé)

| Phase | Contenu | Effort | Dépendances |
|---|---|---|---|
| **P0** | **IPC** : socket Unix JSON entre `zer0` et `zer0chan` (remplace le FIFO) | petit | — |
| **P1** | **Cerveau** : persona + **détection action/émotion/GIF** en JSON structuré | petit | LLM (déjà) |
| **P2** | **Actions** : port des `actions/*.yaml` → Rust, exécution (open/subprocess) | moyen | — |
| **P3** | **Voix** : jouer les WAV existants (`rodio` ou `pw-play`) | petit | — |
| **P4** | **STT** : micro → `whisper-rs` (ou whisper.cpp externe) | moyen | modèle ggml |
| **P5** | **Visage** : option A (egui overlay) **ou** D (panneau TUI animé) | moyen→gros | — |
| **P6** | **Hotkey** : bind Hyprland / evdev | petit | — |

---

## 6. Ma recommandation

1. **P0–P3 d'abord** : c'est là que ZER0v1 gagne (LLM + actions + voix), sans GUI → **valeur immédiate**.
2. Pour le **visage** : commencer par **D (panneau TUI animé)** — zéro risque Wayland, cohérent avec ZER0v1 — puis éventuellement A.
3. **Ne pas** coupler le cerveau à l'avatar : `zer0chan` consomme le cerveau via IPC, comme un client.
4. **TTS** : garder le **pré-enregistré** (les WAV existent) ; la synthèse dynamique est un autre projet.

### Ce qu'il faut décider
- **Langage du service `zer0chan`** : Rust (autonome) ou Python (rapide) ?
- **Visage** : overlay GUI (A) ou **TUI** (D) ?
- **STT** : `whisper-rs` (Rust) ou réutiliser `faster-whisper` (Python) ?
- **Réutilisation des assets** : copier les 12 GIF + WAV dans `ZER0v1/mods/zerochan/` ?
