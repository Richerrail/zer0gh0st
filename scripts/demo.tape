# ZER0│GH0ST — démo automatisée (VHS : github.com/charmbracelet/vhs)
#   ./scripts/demo.sh        (ou)   vhs scripts/demo.tape
#
# NOTE : VHS ne sait pas envoyer les touches F1–F12 ni Escape → on utilise les
# commandes équivalentes (/menu, /readwrite, /codeexec, /chats, /nudge, /0chan, /mic).

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

# ── Menu « Commandes » au clavier (/menu puis ↓ / Entrée) ─
Type "/menu"
Enter
Sleep 800ms
Down 25
Sleep 600ms
Enter
Sleep 800ms
Down 2
Enter
Sleep 2s
Type "/panel"
Enter
Sleep 1s

# ── Raccourcis clavier (équivalents de F3–F10) ────────────
Type "/readwrite"
Enter
Sleep 1s
Type "/codeexec"
Enter
Sleep 1s
Type "/chats"
Enter
Sleep 1500ms
Type "q"
Sleep 1s
Type "/nudge"
Enter
Sleep 3s
Type "/0chan"
Enter
Sleep 1s
Type "/mic"
Enter
Sleep 1s

# ── Une petite tâche (provider/modèle de la config) ───────
Type "En une phrase, à quoi sert ce projet ?"
Enter
Sleep 25s

# ── Quitter ───────────────────────────────────────────────
Type "/quit"
Enter
Sleep 1s
