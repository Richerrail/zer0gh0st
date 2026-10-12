# ZER0│GH0ST — démo automatisée (VHS : github.com/charmbracelet/vhs)
# Génère un GIF/MP4 en rejouant les commandes, les raccourcis et une tâche.
#   ./scripts/demo.sh           (ou)   vhs scripts/demo.tape
#
# Le provider/modèle utilisé est celui de la config au démarrage.

Output demo.gif
Set Shell bash
Set FontSize 14
Set FontFamily "JetBrains Mono"
Set Width 1400
Set Height 900
Set Padding 10
Set TypingSpeed 35ms

# ── Lancement ─────────────────────────────────────────────
Type "./target/release/zer0 --allow-exec"
Enter
Sleep 3s

# ── Commandes ─────────────────────────────────────────────
Type "/help"
Enter
Sleep 3s
Type "/kb stats"
Enter
Sleep 1200ms
Type "/task list"
Enter
Sleep 1200ms
Type "/memory"
Enter
Sleep 1200ms
Type "/ctx"
Enter
Sleep 800ms
Type "/provider list"
Enter
Sleep 1200ms
Type "/model"
Enter
Sleep 800ms
Type "/route list"
Enter
Sleep 1200ms
Type "/role list"
Enter
Sleep 1200ms
Type "/think"
Enter
Sleep 800ms
Type "/whoami"
Enter
Sleep 1200ms

# ── Menu cliquable au clavier : F2 puis ↑↓ / Entrée ───────
Key F2
Sleep 1s
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Down
Key Enter
Sleep 1s
Key Down
Key Enter
Sleep 2s
Key Escape
Sleep 500ms
Type "/panel"
Enter
Sleep 1s

# ── Raccourcis clavier ────────────────────────────────────
Key F3
Sleep 1s
Key F4
Sleep 1s
Key F5
Sleep 2s
Key F6
Sleep 2s
Key Escape
Sleep 1s
Key F7
Sleep 3s
Key F8
Sleep 1s
Key F10
Sleep 1s

# ── Une petite tâche (provider/modèle de la config) ───────
Type "En une phrase, à quoi sert ce projet ?"
Enter
Sleep 25s

# ── Un module fun ─────────────────────────────────────────
Type "/bintime"
Enter
Sleep 2s
Key Escape
Sleep 500ms

# ── Quitter ───────────────────────────────────────────────
Type "/quit"
Enter
Sleep 1s
