# Contribuer à ZER0│GH0ST

Merci de ton intérêt. Le projet est un **agent IA en terminal** (Rust, un seul binaire),
et le principe directeur est simple :

> **Ne jamais prétendre qu'une chose est sûre ou fiable quand elle ne l'est pas.**

Toute contribution est bienvenue — tant qu'elle respecte ce principe.

## Mise en route

```bash
git clone https://github.com/Richerrail/zer0gh0st.git
cd zer0gh0st
cargo build --release          # ou ./scripts/install.sh
```

Prérequis : **Rust stable** + un compilateur C (`cc` — `rusqlite` est compilé *bundled*).
`llama-server` (llama.cpp) n'est nécessaire que pour l'inférence **locale**.

## Avant de pousser

```bash
cargo clippy --all-targets --features gateway -- -D warnings   # zéro warning exigé
cargo test --all-targets --features gateway                     # tests verts
cargo build --release
```

La CI fait exactement cela sur **Linux, macOS et Windows**. Si le clippy échoue, la CI
échoue : c'est volontaire (le code est maintenu sans warning).

## Style

- `cargo fmt` (le formatage est informatif en CI, mais reste le bienvenu).
- Commentaires **en français**, comme le reste du code ; explique le **pourquoi**, pas le
  **quoi**.
- Pas de dépendance heavier sans raison : le binaire doit rester léger et autonome.
- Les secrets ne vont **jamais** dans le dépôt (ni dans une config versionnée).

## Format des commits

Courts et explicites :

```
feat(tui): menu de sélection /model et /provider
fix(zer0-gw): ne pas dupliquer le segment de version de l'URL (z.ai 404)
docs(README): préciser le garde-fou « gateway »
```

## Pull requests

1. Fork + branche (`git checkout -b feat/ma-feature`).
2. Commit(s) clairs.
3. PR qui explique **le pourquoi** (pas seulement le quoi) et ce qui a été testé.
4. La checklist ci-dessous.

### Checklist

- [ ] `cargo clippy --all-targets --features gateway -- -D warnings` passe
- [ ] `cargo test --all-targets --features gateway` passe
- [ ] La doc est à jour (`README.md`, `CHANGELOG.md`, et `SAFETY.md` si la sécurité change)
- [ ] Les **limites** de ce qui est ajouté sont documentées (principe directeur)
- [ ] Aucun secret, aucune donnée personnelle dans le diff
- [ ] Les fichiers volumineux (modèles, médias, données) ne sont pas commités

## Idées

Une idée est bienvenue : ouvre une *issue* ou une *discussion*. Les limites connues et
ce qui n'est pas couvert sont décrits dans [`SAFETY.md`](./SAFETY.md).

## Sécurité

Ne signale **pas** une faille par une issue publique : vois [SECURITY.md](./SECURITY.md).
