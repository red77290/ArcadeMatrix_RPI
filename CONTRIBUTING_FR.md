🇬🇧 [English](CONTRIBUTING.md) | 🇫🇷 Français | 🇪🇸 [Español](CONTRIBUTING_ES.md)

# Contribuer à ArcadeMatrix

Bienvenue dans ArcadeMatrix ! Que vous corrigiez des bugs, ajoutiez de nouvelles animations ou portiez ArcadeMatrix sur une nouvelle plateforme, nous sommes ravis de vous accueillir.

Ce document présente la philosophie, l'architecture et les conventions de code du projet afin de garantir une base de code robuste et maintenable à mesure que le projet grandit.

## Philosophie du projet

Le projet ArcadeMatrix est divisé en trois phases d'évolution distinctes :
1. **Prototype** : faire fonctionner l'affichage.
2. **Produit** : ajouter des fonctionnalités (GIFs, weather, clock, WebUI, MQTT...).
3. **Framework** : stabiliser l'architecture, les tests et la rendre extensible.

Nous sommes actuellement dans la phase **Framework**. Toute nouvelle contribution doit respecter une séparation stricte des responsabilités, en évitant le « spaghetti code » et les moteurs monolithiques.

## Séparation des responsabilités : le Rendering Pipeline

Nous utilisons un Rendering Pipeline moderne pour gérer ce qui est dessiné sur la matrice. Si vous souhaitez ajouter un nouvel effet visuel ou une nouvelle manière d'afficher l'heure/la date, veuillez suivre ce flux :

`Data -> Engine -> Animation -> Renderer -> Matrix`

### Engines vs. Renderers

* **Engine (`engines/`)** : responsable de l'acquisition des données, de la gestion d'état et de la logique métier.
  * *Exemple* : `ClockEngine` sait *quelle heure il est* et *quand effectuer la rotation*. Il ne sait **pas** dessiner une boîte blanche qui rétrécit.
  * *Exemple* : `WeatherEngine` sait interroger l'API et parser le JSON.
* **Renderer (`engines/renderers/`)** : responsable du dessin des pixels dans la frame. Il n'a AUCUNE logique métier. Il prend des chaînes de données brutes, des polices et des couleurs, puis renvoie une image.
  * *Exemple* : `CyberpunkRenderer` sait dessiner une pluie numérique verte qui tombe.
  * *Exemple* : `FlipRenderer` sait calculer des bounding boxes et dessiner des panneaux qui se rétractent.

**Règle pratique :**
Si vous ajoutez un nouveau thème visuel qui utilise les *mêmes données* (comme un nouveau cadran d'horloge), créez un **Renderer** (ou une horloge spécialisée comme `PongClock`, qui agit comme un renderer).
Si vous ajoutez une fonctionnalité complètement nouvelle (comme la récupération de cours boursiers ou Spotify now playing), créez un **Engine**.

## Conventions de code

* **Langage** : le dépôt principal utilise Rust pour le Raspberry Pi.
* **Typage** : utilisez pleinement le typage statique fort de Rust et ses Traits pour clarifier les contrats Engine/Renderer.
* **Tests** : toutes les routes API et la logique de configuration Core doivent être couvertes par `cargo test`.
* **Indépendance matérielle** : ne supposez pas que la matrice fait exactement 64x32. Utilisez toujours `self.config.matrix_width` et `self.config.matrix_height`.

## Ajouter un nouveau Renderer

*Note : La procédure exacte est en cours de mise à jour pour l'architecture Rust.*
1. Créez un nouveau fichier dans `src/engines/renderers/my_custom_renderer.rs`.
2. Implémentez le trait `Renderer`.
3. Enregistrez-le dans `src/engines/renderers/mod.rs`.

## Architecture Decision Records (ADR)

Si vous proposez un changement d'architecture majeur, veuillez rédiger un ADR dans `docs/adr/`. Consultez les ADR existants pour comprendre pourquoi certaines décisions d'architecture (comme l'évitement du multi-threading pour le rendu) ont été prises.
