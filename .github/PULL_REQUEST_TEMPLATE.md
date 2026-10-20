<!--
Merci pour la contribution ! Le principe directeur du projet :
« Ne jamais prétendre qu'une chose est sûre ou fiable quand elle ne l'est pas. »
Donc : documente aussi ce que ta PR **ne** fait pas (ou ne garantit pas).
-->

## Ce que ça change

<!-- Le problème résolu, en 2-3 phrases. -->

## Pourquoi

<!-- Le contexte : pourquoi cette solution plutôt qu'une autre. -->

## Type de changement

- [ ] Correction de bug
- [ ] Nouvelle fonctionnalité
- [ ] Durcissement / sécurité
- [ ] Documentation
- [ ] Refactor / technique

## Vérifications

- [ ] `cargo clippy --all-targets --features gateway -- -D warnings` passe
- [ ] `cargo test --all-targets --features gateway` passe
- [ ] Testé en vrai (TUI et/ou CLI) — décris ci-dessous ce que tu as fait

```
(exemple : zer0 --once "…", puis /apis crypto dans la TUI)
```

## Documentation

- [ ] `README.md` / `CHANGELOG.md` mis à jour
- [ ] `SAFETY.md` mis à jour si la sécurité change
- [ ] **Limites documentées** : ce que cette PR ne couvre pas, ce qui peut échouer

## Notes

<!-- Screenshots, logs, contexte de configuration… -->
