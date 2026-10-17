🇬🇧 [English](ARCHITECTURE_PARITY.md) | 🇫🇷 Français | 🇪🇸 [Español](ARCHITECTURE_PARITY_ES.md)

# Parité Architecturale : ESP32 Core 1 vs RPi H-Core

Ce document formalise la parité architecturale normative et les garanties temps réel partagées entre **ArcadeMatrix (ESP32 / C++)** et **ArcadeMatrix_RPi (Raspberry Pi / Rust)**.

---

## 1. Synthèse Exécutive & Invariants de Conception

Les deux plateformes adhèrent au modèle d'exécution **V1.6-FinalLocked**, imposant une séparation stricte entre le **Plan de Contrôle (Core 0 / Async Tokio)** et le **Plan de Rendu Temps Réel (Core 1 / Thread H-Core)**.

| Invariant Architectural | Implémentation ESP32 (C++) | Implémentation RPi (Rust) |
| :--- | :--- | :--- |
| **Allocation Mémoire Hot-Path** | **Strictement Zéro-Allocation** (`malloc`, `new`, `std::vector`, `String` interdits) | **Strictement Zéro-Allocation** (`Vec::push`, `String`, `format!`, `serde_json`, `HashMap` interdits) |
| **Absence de Verrou Hot-Path** | **Zéro Mutex** sur le hot-path Core 1. Triple Buffer SRSW lock-free CAS. | **Zéro Mutex** sur la boucle de rendu H-Core. Frontière de publication atomique `ArcSwap<ProducerSnapshot>`. |
| **Identité d'Intention** | `source_id + request_id + engine_handle` | `source_id + request_id + engine_handle` (Uniforme pour toutes les sources, zéro exception) |
| **Modèle de Slots de l'Arbitre** | 8 slots privés, consommation file SPSC en $O(1)$ | 8 slots bornés, balayage linéaire en $O(1)$ |
| **Pile de Préemption** | `PreemptionStack<PreemptionEntry, 4>` fixe | `[PreemptionEntry; 4]` borné, `depth: usize` |
| **Saturation de Préemption** | Rejet strict à `depth == 4` (session active préservée) | Rejet strict à `depth == 4` (session active préservée) |
| **Résolution de Handle** | `EngineHandle` POD $O(1)$ (`uint16_t descriptorId`, `uint16_t instanceId`) | Indexation directe $O(1)$ dans `Vec<Option<RegisteredInstance>>` |

---

## 2. Identité d'Intention & Machine à États du Cycle de Vie

Une intention de requête d'affichage est identifiée de manière unique par le triplet :
$$\text{Intent} = (\text{source\_id}, \text{request\_id}, \text{engine\_handle})$$

### Table de Vérité & Matrice de Transition

| Scénario | Décision de l'Arbitre | État de la Session Active | `TransitionMode` Résultant | Action FSM |
| :--- | :--- | :--- | :--- | :--- |
| **Rotation de Base** | `Rotation` (P10) | Aucune | `TransitionMode::Replace` | `activate(rot)` |
| **Préemption Prioritaire** | `Mqtt` (P40, préemptif) | `Rotation` (P10) | `TransitionMode::Preempt` | `pause(rot) -> push(rot) -> activate(mqtt)` |
| **Saturation de la Pile** | `Mqtt` (P60, préemptif) | `depth == 4` | `TransitionMode::None` | Requête rejetée ; la session active continue |
| **Rafraîchissement Même Intention** | Même `(source, req_id, handle)` | Active | `TransitionMode::None` | NOOP interne |
| **Mise à Jour Interne de Payload** | Même `source` & `handle`, nouveau `req_id` | Active | `TransitionMode::None` | Mise à jour interne sans empilement sur la pile de préemption |
| **Fin de Préemption (Base Valide)** | `Rotation` (P10) | `Mqtt` actif, pile contient `Rotation` | `TransitionMode::Resume` | `deactivate(mqtt) -> pop() -> resume(rot)` |
| **Expiration d'Orphelin Submergé** | Session intermédiaire annulée pendant sa submersion | Pile multi-niveaux | `TransitionMode::Resume` | Dépile les sessions intermédiaires expirées, reprend directement l'ancêtre valide |
| **Épuisement de la Pile** | Aucune (`DisplayDecision::NONE`) | Pile vide | `TransitionMode::None` | `deactivate()` tout, retourne `None` |
| **Handle de Moteur Invalide** | `handle` inconnu | Quelconque | `TransitionMode::None` | Transaction rejetée ; session active intacte |

---

## 3. Frontière Temps Réel (Zéro-Allocation & Zéro-Mutex)

### 3.1 Plan de Contrôle (Async Tokio / Core 0)
- Responsable de l'API REST HTTP, des souscriptions MQTT et de la validation JSON de la configuration.
- Pré-analyse les charges utiles entrantes dans des structures fortement typées (ex. `MessagePayload`).
- Publie les modifications de manière atomique en mémoire partagée via `arc_swap::ArcSwap<ProducerSnapshot>` sans aucune acquisition de verrou sur le thread temps réel.

### 3.2 Plan de Rendu Temps Réel (H-Core / Core 1)
- Consomme les charges utiles pré-analysées et les instantanés de configuration via des lectures atomiques de pointeurs (`producer_snapshot.load()`).
- Zéro verrou mutex sur l'intégralité du hot-path de la boucle de rendu.
- Exécute `DisplayArbiter::evaluate()` en complexité temporelle $O(1)$ sans allocation dynamique.
- Résout les handles de moteurs en temps $O(1)$ via des offsets de vecteur indexés (`handle.instance_id as usize`).
- Distribue les transitions de cycle de vie (`activate`, `deactivate`, `pause`, `resume`) et invoque `update()` & `render()`.
- Vérifié par les tests unitaires avec `GlobalAlloc` instrumenté : **0 allocation sur le tas sur 10 000 itérations en régime permanent**.

---

## 4. Différences Intentionnelles de Plateforme

| Dimension | ESP32 (FreeRTOS Embarqué / C++) | Raspberry Pi (Linux Embarqué / Rust) | Justification |
| :--- | :--- | :--- | :--- |
| **Driver Matériel Matrice** | DMA parallèle I2S ESP32 / Périphérique LCD ESP32-S3 | Bindings C `rpi-rgb-led-matrix` / Backend Mock | Le RPi utilise le mappage mémoire GPIO direct avec timers PWM matériels |
| **Enregistrement des Plugins** | Linker sets C++ (`ENGINE_REGISTER`) | `linkme::distributed_slice` (`ENGINES`) | Enregistrement idiomatique de plugins à la compilation |
| **Modèle de Threading** | Épinglage de tâches FreeRTOS dual-core (`xTaskCreatePinnedToCore`) | Threads OS : runtime Tokio + thread de rendu temps réel (`std::thread::spawn`) | Modèle de concurrence POSIX standard sous Linux |
| **Triple Buffer de Configuration** | Triple-buffer SRSW lock-free avec atomiques & double magic CRC | Instantanés immuables pré-construits & `ArcSwap` / drapeaux atomiques | Exploite la sûreté mémoire de Rust tout en garantissant une exécution lecteur non-bloquante |

---

## 5. Suite de Tests Automatisés de Vérification & Conformité

La suite de conformité normative est codifiée dans :
1. `tests/test_esp_alignment.rs` : Valide la parité comportementale avec l'architecture de référence ESP32.
2. `tests/test_v16_final_locked.rs` : Implémente un `TrackingAllocator` instrumenté fournissant la preuve mécanique de 0 allocation sur le tas dans le hot-path, la vérification complète de la matrice FSM, la saturation de la pile bornée et la sûreté du registre.

