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
* **Indépendance matérielle** : ne supposez pas que la matrice fait exactement 64x32. Lisez la taille du panneau depuis `MatrixConfig` (`matrix.width` / `matrix.height`) et déclarez les résolutions supportées via les `Capabilities` du descripteur (`supports_128x32` / `supports_256x64`).

## L'architecture des moteurs (Registry / Descriptor / Factory)

Le Core est **agnostique des moteurs** : il ne nomme jamais directement `Clock`,
`Weather` ou `Spotify`. Chaque moteur est un plugin auto-décrit, découvert à
l'exécution :

```
Engine
 ├── Descriptor  (métadonnées + Capabilities + Requirements)
 ├── ConfigSchema (champs, types, valeurs par défaut, min/max, options, options_endpoint)
 ├── Factory     (construction paresseuse, créé une seule fois puis mis en cache)
 └── Lifecycle   (initialize → activate → update/render → deactivate)
```

- Les moteurs sont configurés comme des **instances génériques** (`instance_id`
  + `engine_id` + une `config` sous forme de map de chaînes), et non comme des
  types codés en dur.
- L'interface Web est générée depuis `GET /api/engines` : un nouveau moteur
  apparaît automatiquement dans l'UI dès que son `ConfigSchema` est déclaré,
  sans modifier le frontend.
- Les modifications de config atteignent un moteur en cours d'exécution en
  direct via `on_config_changed()` ; la config est auto-réparée (valeurs par
  défaut injectées, valeurs hors bornes clampées ou réinitialisées) avant d'être
  persistée.

Le guide complet et de référence se trouve dans
[`docs/DEVELOPER_FR.md`](docs/DEVELOPER_FR.md).

## Ajouter un nouveau moteur

1. Créez `src/engines/my_engine.rs` implémentant le contrat `Engine`
   (`initialize` / `activate` / `update` / `render` / `deactivate`, plus
   `is_finished` et `on_config_changed` si nécessaire).
2. Fournissez un `EngineDescriptor` : métadonnées, `Capabilities` (mettez
   `realtime: true` uniquement si le moteur doit se mettre à jour à chaque
   frame), `Requirements` et un `ConfigSchema`.
3. Enregistrez-le dans le registre via l'entrée factory
   `#[distributed_slice(ENGINES)]` pour que l'auto-découverte et l'UI le
   prennent en compte.
4. Ajoutez des tests dans `tests/` (voir `tests/test_registry.rs` et
   `tests/test_sanitizer.rs`).

## Ajouter un nouveau Renderer

Si vous avez seulement besoin d'un nouveau *rendu visuel* pour des données
existantes (ex. un nouveau cadran d'horloge), ajoutez un **Renderer** plutôt
qu'un moteur :
1. Créez un nouveau fichier dans `src/engines/renderers/my_custom_renderer.rs`.
2. Implémentez le trait `Renderer`.
3. Enregistrez-le dans `src/engines/renderers/mod.rs`.

## Architecture Decision Records (ADR)

Si vous proposez un changement d'architecture majeur, veuillez rédiger un ADR dans `docs/adr/`. Consultez les ADR existants pour comprendre pourquoi certaines décisions d'architecture (comme l'évitement du multi-threading pour le rendu) ont été prises.
