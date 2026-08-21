🇬🇧 [English](ARCHITECTURE.md) | 🇫🇷 Français | 🇪🇸 [Español](ARCHITECTURE_ES.md)

# Aperçu de l'Architecture (Raspberry Pi - Rust)

Ce document fournit une vue d'ensemble détaillée de l'architecture d'ArcadeMatrix sur Raspberry Pi développé en **Rust**. Il explique les choix de conception profonds, la stratégie mémoire, le pipeline de rendu et le cycle de vie "Lazy-Once" des moteurs.

---

## 1. Philosophie de conception : Performances et "Jitter"

Contrairement à l'ESP32, le Raspberry Pi dispose d'une mémoire vive abondante (512 Mo à 8 Go). Cependant, son système d'exploitation n'est pas "Temps Réel" (RTOS). Le driver de la matrice (via DMA/GPIO) est extrêmement sensible aux micro-coupures ("jitter"). 

Pour conserver un taux de rafraîchissement stable de 60 FPS sans déchirement de l'image (tearing), **le hot loop (`update()` et `render()`) ne doit générer aucune allocation dynamique inutile**. Les allocations entraînent un travail de nettoyage ou de redimensionnement de heap (tas) qui peut introduire une latence imprévisible de quelques millisecondes, suffisante pour faire scintiller la matrice LED.

---

## 2. Le Cycle de Vie "Lazy-Once"

Pour répondre à cette contrainte, l'architecture repose sur un modèle de cycle de vie très strict appelé **Lazy-Once**.

```mermaid
graph TD
                 Registry[Engine Registry]
                       │
                 Descriptor[EngineDescriptor]
                       │
                    Factory[Factory]
                       │
                 Instance[EngineInstance]
                       │
              ┌────────┴────────┐
              │                 │
        Context[EngineContext] Config[EngineConfig]
              │                 │
              └────────┬────────┘
                       │
                 Runtime[Engine Runtime]
                       │
          ┌────────────┼────────────┐
          │            │            │
       activate      update       render
          │            │            │
          └────────────┼────────────┘
                       │
                  deactivate
```

### Explication des phases :

1. **`initialize()` (Allocation) :** 
   * **Quand ?** Appelée *exactement une fois* dans toute la vie du programme, la première fois que le moteur doit être affiché (instanciation paresseuse "Lazy").
   * **Pourquoi ?** Permet d'éviter de charger en RAM des assets (images, polices) pour des moteurs que l'utilisateur a désactivés dans la configuration. C'est ici que l'on charge les bitmaps et qu'on prépare le terrain de jeu.
2. **`activate()` (Préparation temporaire) :** 
   * **Quand ?** Appelée à chaque fois que le moteur devient le moteur "actif" à l'écran.
   * **Pourquoi ?** Permet de réinitialiser l'état (par exemple, remettre la balle de Pong au centre, ou relancer un chronomètre) sans avoir à réallouer la mémoire.
3. **`update()` & `render()` (Hot Loop - 60 FPS) :**
   * **Contrainte :** **Aucune allocation dynamique inutile.** La mémoire nécessaire (String, Vec) doit avoir été réservée dans `initialize` ou réutilisée (ex: `String::clear()` puis `write!()` au lieu d'allouer de nouvelles chaînes).
4. **`deactivate()` (Mise en veille) :**
   * Permet d'arrêter des tâches de fond lourdes quand le moteur n'est plus à l'écran.
5. **`is_finished()` (Saut conditionnel) :**
   * Permet au moteur de signaler au `Runtime` de rotation qu'il a fini sa tâche (ex: le Moteur Crypto a fini d'afficher tous ses jetons).

---

## 3. Découplage : Registry et Configuration

### Pourquoi le Core ne contient-il pas une liste de types concrets ?
Dans les versions précédentes, `app.rs` incluait manuellement tous les fichiers d'horloges et créait un énorme bloc `match` avec des `Box::new(ClockEngine)`. Cela cassait le principe ouvert/fermé (SOLID) : pour ajouter un moteur, il fallait modifier le cœur de l'application.
Grâce au **Registry** (basé sur la macro `#[distributed_slice]`), chaque moteur s'enregistre de manière autonome lors de la compilation. Le Core de l'application ignore totalement l'existence des moteurs concrets.

### Pourquoi le Registry contient-il des descripteurs plutôt que des instances ?
L'instanciation immédiate de tous les moteurs au démarrage (`Box::new(...)`) consommerait inutilement la RAM et ralentirait le boot. Le descripteur stocke plutôt une **Factory** (une fonction pointeur créant l'instance à la volée) et les métadonnées requises.

### Pourquoi séparer `config.json` et `EngineConfig` ?
Le fichier racine (`config.json`) décrit l'ensemble de l'appareil (WiFi, Matrice, etc.). Cependant, les moteurs n'ont pas besoin — et ne doivent pas avoir accès — à la configuration du WiFi ou d'autres moteurs. `EngineConfig` agit comme une vue ou un proxy restreint fournissant uniquement les variables déclarées par le moteur via son `ConfigSchema`.

---

## 4. Isolation du Runtime & Modèle de Threading

ArcadeMatrix s'appuie sur une architecture multi-threads pour isoler le rendu matériel des opérations réseau :

1. **Thread de Rendu Dédié (`matrix-render`) :**
   - S'exécute dans un thread OS dédié avec une pile de 8 Mo.
   - Accès exclusif à la matrice LED. S'il était combiné avec l'API Web, chaque requête HTTP provoquerait un saut de trame (tearing) sur la matrice.

2. **Thread API Web Isolé (`api-server`) :**
   - Tourne sur un runtime Tokio mono-threadé (`Builder::new_current_thread()`).
   - Gère le paramétrage par l'interface web (port 80). Communique avec le thread de rendu uniquement via des primitives atomiques (`AtomicBool`) ou des serrures asynchrones (`RwLock`) de courte durée.

3. **Services d'Arrière-plan :**
   - **Écouteur MQTT / APIs HTTP :** Isolés pour ne jamais bloquer le calcul des frames (`update()`).
