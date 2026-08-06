🇬🇧 [English](GETTING_STARTED.md) | 🇫🇷 Français | 🇪🇸 [Español](GETTING_STARTED_ES.md)

# Premiers pas (app Raspberry Pi en Rust, configuration du workspace développeur)

Ce guide s'adresse aux développeurs qui mettent en place un **environnement de développement local** sur leur propre machine (Mac/Linux/Windows) pour travailler sur la codebase ArcadeMatrix_RPi en **Rust natif**.

---

## 1. Prérequis système

- **Rust Toolchain (1.75+)** : installable via `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Cargo** : le gestionnaire de paquets et de build Rust (fourni avec Rustup).

---

## 2. Compiler et exécuter en local (Dev / Mock Matrix)

Sur n'importe quel Mac, Linux ou Windows sans Raspberry Pi :

```bash
git clone <this-repo-url>
cd ArcadeMatrix_RPi
git checkout rust_migration

# Vérification rapide de compilation
cargo check

# Compiler et lancer en mode de développement avec le Mock Canvas
cargo run
```

Par défaut sur Mac/Windows, le projet utilise `MockMatrix`, qui simule la matrice LED en mémoire tout en lançant le serveur Web Actix à l'adresse `http://127.0.0.1:8080`.

---

## 3. Exécuter la suite de tests

La suite de tests Rust valide la configuration, l'API REST Actix et la validation des binaires de mise à jour OTA (`POST /api/update`) :

```bash
cargo test
```

Vérification du formatage et des règles de linter :

```bash
cargo fmt --check
cargo clippy -- -D warnings
```

---

## 4. Cross-compilation et Déploiement Raspberry Pi

Pour compiler le binaire natif pour Raspberry Pi depuis votre machine Mac/Linux :

```bash
# Installer cross
cargo install cross

# Cross-compilation 64-bit ARM (Raspberry Pi 3, 4, Zero 2 W)
cross build --target aarch64-unknown-linux-gnu --release

# Cross-compilation 32-bit ARM (Raspberry Pi 2, Zero)
cross build --target armv7-unknown-linux-gnueabihf --release
```

Le binaire produit se trouve dans `target/aarch64-unknown-linux-gnu/release/arcadematrix`. Il peut être déployé directement sur le Pi ou mis à jour sans interruption via l'interface Web (section **Firmware Update (OTA)**).
