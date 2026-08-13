🇬🇧 [English](ARCHITECTURE.md) | 🇫🇷 Français | 🇪🇸 [Español](ARCHITECTURE_ES.md)

# Aperçu de l'Architecture (Raspberry Pi - Rust)

Ce document fournit une vue d'ensemble détaillée de l'architecture d'ArcadeMatrix sur Raspberry Pi développé en **Rust**. Il explique les choix de conception principaux, le pipeline de rendu, l'isolation des runtimes multi-threads, l'injection de dépendances et les mécanismes de rotation matérielle.

---

## 1. Philosophie de base

ArcadeMatrix est un exécutable Rust binaire natif conçu pour piloter une matrice LED HUB75 à l'aide des bindings `rpi-led-matrix-sys` de la bibliothèque C++ `hzeller/rpi-rgb-led-matrix`. Les objectifs principaux sont :
- **Rendu pixel-perfect :** Prise en charge des polices bitmap `.bdf` nettes et des sprites RGB.
- **Modularité :** Ajout facile de nouveaux thèmes visuels, horloges et sources de données via des traits Rust.
- **Réactivité :** Serveur Web API léger (`actix-web`) s'exécutant de manière isolée sans perturber le driver matériel de la matrice.

---

## 2. Le Pipeline de Rendu

Pour préserver la maintenabilité du code, la logique *du contenu* est strictement séparée *de la méthode de dessin*.

### Diagramme de haut niveau

```mermaid
graph TD
    subgraph Data & API Layer
        API[Actix-web REST API]
        Config[conf.ini / ConfigLoader]
        Time[Chrono System Time]
        Network[OpenWeather / MQTT / Crypto / Stock APIs]
    end

    subgraph Engine Layer (Rust)
        App[ArcadeMatrixApp]
        Rot[RotationState]
        ClockE[ClockEngine]
        DateE[DateEngine]
        WeathE[WeatherEngine]
        CryptoE[CryptoEngine]
        StockE[StockEngine]
        Rot --> ClockE & DateE & WeathE & CryptoE & StockE
    end

    subgraph Logic & Aesthetic Layer
        ClockE -->|Theme ID 0-21| Renderers[Base, Cyberpunk, Flip, TrueMatrix]
        ClockE -->|Theme ID 22+| SpClocks[Pong, Tetris, Pacman, Versus, SlotMachine]
        Renderers --> Pil[image-rs Canvas]
        SpClocks --> Pil
    end

    subgraph Hardware & Mock Layer
        Pil --> Wrapper[HardwareMatrix / MockMatrix]
        Wrapper --> Hardware[Matrice LED HUB75]
    end

    API -.->|Writes INI| Config
    Config -.->|Signals| Rot
```

---

## 3. Isolation du Runtime & Modèle de Threading

ArcadeMatrix RPi s'appuie sur une architecture multi-threads pour isoler le rendu matériel des opérations réseau d'E/S asynchrones :

1. **Thread de Rendu Dédié (`matrix-render`) :**
   - S'exécute dans un thread OS dédié avec une pile de 8 Mo.
   - Boucle de rendu matrice haute fréquence. Réessaie l'initialisation matérielle si le GPIO/DMA est verrouillé par un processus précédent.

2. **Thread API Web Isolé (`api-server`) :**
   - Tourne sur un runtime Tokio mono-threadé (`Builder::new_current_thread()`).
   - Empêche les requêtes réseau de spawner des threads sur tous les cœurs CPU et de provoquer la saturation des interruptions Wi-Fi.
   - Communique avec le thread de rendu uniquement via des drapeaux atomiques (`reload_flag`, `reset_rotation`, `matrix_power`) et `RwLock<ConfigSettings>`.

3. **Services d'Arrière-plan :**
   - **Écouteur MQTT (`rumqttc`) :** Reçoit les événements de jeu depuis Batocera / Recalbox.
   - **Engines Multi-Fournisseurs :** Récupération de cours d'actifs Crypto (CoinGecko, Binance), Bourse (Yahoo Finance) et Météo (OpenWeatherMap).

---

## 4. Comptage des Symboles & Saut Automatique en Rotation

Le moteur de rotation gère l'affichage des jetons (`crypto`, `stocks`) de manière dynamique :
- **Parsing des symboles :** Les chaînes de symboles séparées par des virgules sont nettoyées et filtrées pour exclure les espaces et jetons vides.
- **Saut automatique :** Si le nombre de symboles valides pour `crypto` ou `stocks` est égal à `0` (ou si le module est désactivé), `RotationState` passe immédiatement au module suivant dans la playlist sans délai.
- **Durée dynamique :** La durée d'affichage s'adapte au nombre de symboles actifs (`durée = nombre_symboles * 5s`).

---

## 5. Différences d'Architecture RPi (Rust) vs ESP32 (C++)

- **RPi (Rust) :** Utilise un pipeline de rendu découplé (Engines -> Renderers -> Canvas `image-rs` -> Matrice). La RAM abondante (512MB+) permet de composer les frames complètes avant transfert vers `rpi-rgb-led-matrix`.
- **ESP32 (C++) :** Utilise une structure de rendu DMA direct. La RAM est limitée à 320 Ko. Les primitives dessinent directement dans les buffers DMA avec un surcoût mémoire minimal.

---

## 6. Injection de Dépendances & Traits

Les moteurs de données s'appuient sur des interfaces `trait` Rust (`IProvider`) :
- `CryptoEngine` supporte plusieurs fournisseurs (`CoinGeckoProvider`, `BinanceProvider`).
- `StockEngine` supporte `StockProvider` (`YahooFinanceProvider`).
- `WeatherEngine` supporte `WeatherProvider` (`OpenWeatherMapProvider`).
