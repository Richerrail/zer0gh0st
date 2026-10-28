# Suggestions d'amélioration (hors périmètre actuel)

Ces idées sont **volontairement non implémentées**. Elles sont notées ici pour
ne pas les perdre, avec leurs contraintes — pas comme une feuille de route.

## TTS — sortie vocale (M8 de l'audit)

**Pourquoi pas maintenant** : ajoute une dépendance système, un choix de voix,
et un risque de latence/conflit avec le streaming. L'entrée audio (Zero-chan +
Whisper) existe déjà ; la sortie est un autre chantier.

Pistes, de la plus légère à la plus lourde :

| Option | Poids | Qualité | Notes |
|---|---|---|---|
| `espeak-ng` | ~2 Mo | robotique | dispo partout, trivial à appeler |
| `piper` | ~20 Mo + voix | bonne, locale | voix FR à télécharger, offline |
| API cloud (ElevenLabs…) | 0 local | excellente | clé + coût + vie privée |

Points d'intégration possibles :

- commande `/say <texte>` (lire la dernière réponse) ;
- outil `speak(text)` (cloud agent), avec plafond de caractères ;
- Zero-chan : mode « conversation vocale » boucle STT → LLM → TTS.

Garde-fous à prévoir : couper la parole à `/stop`, ne jamais lire un contenu
marqué secret, ne pas démarrer deux lectures en parallèle, et rester muet si
le binaire TTS est absent (message explicite, pas de fallback silencieux).

## Vision multimodale (reste de M1)

`read_image` fait de l'**OCR** (texte dans l'image). La vision réelle — décrire
une capture, repérer un alignement cassé — demande :

1. un provider/modèle multimodal (cloud) ;
2. d'injecter l'image comme **résultat d'outil**, ce que l'API OpenAI ne permet
   pas (les résultats d'outils sont du texte) : il faudrait ajouter un message
   `user` avec `image_url` en cours de tour, donc modifier `run_turn` ;
3. un garde-fou de taille (base64) et un refus clair si le modèle n'est pas
   multimodal.

## Divers

- `knowledge_delete` par `source` (l'API existe, l'outil non) ;
- exécution planifiée des tâches (M9 ne fait qu'un **rappel**) : nécessiterait
  un démon, donc un choix d'architecture ;
- échéances en date absolue (`2026-11-02 09:00`) en plus de `+2h` ;
- `read_image` : installer `tesseract-data-fra` pour le français.
