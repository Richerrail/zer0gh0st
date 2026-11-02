# SFX — pack `castle`

Sons d'interface pour le thème `castle` (ZER0│GH0ST).

## Origine et licence

Ces WAV sont **originaux**, synthétisés par `scripts/gen-sfx.py` (ondes
carrées, bruit, enveloppes) — aucun sample tiers, aucune licence à respecter.

Ils sont dédiés au **domaine public (CC0 1.0)** : utilisation, modification et
redistribution libres, sans attribution obligatoire.

## Fichiers

| Fichier | Rôle |
|---|---|
| `tick.wav` | navigation dans les listes |
| `select.wav` | validation |
| `theme_castle.wav` | signature du thème castle |
| `panel_open.wav` | ouverture d'un panneau/mod |
| `toggle_on.wav` / `toggle_off.wav` | activation/désactivation des sons |
| `error.wav` | erreur de tour |
| `done.wav` | fin de réponse |

Format : WAV PCM 16 bits, mono, 44,1 kHz. Volume perçu bas (≈ 28 %).

## Régénérer

```bash
python3 scripts/gen-sfx.py
```

## Activation

Les sons sont **désactivés par défaut** :

```
/sound on
```

ou `ZER0_SOUND` n'est pas utilisé : le réglage est `sound = true` dans la
config globale (`/sound on` l'écrit).
