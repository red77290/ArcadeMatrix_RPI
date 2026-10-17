🇬🇧 [English](SCRIPTS.md) | 🇫🇷 Français | 🇪🇸 [Español](SCRIPTS_ES.md)

# Documentation des Scripts ArcadeMatrix

Ce document explique l'objectif, les prérequis et l'utilisation de chaque script du dossier `scripts/`. Ils permettent de **compiler**, **déployer**, **générer une image SD**, **diagnostiquer** et **valider** ArcadeMatrix_RPi sur toutes les plateformes.

## Prérequis

### 1. Docker
Toute la compilation croisée et la création d'images passent par Docker, vous n'avez donc jamais besoin d'installer une chaîne d'outils ARM localement.
- **macOS / Windows** : [Docker Desktop](https://www.docker.com/products/docker-desktop/) ou Rancher Desktop.
- **Linux** : Docker Engine (`sudo apt install docker.io`).

### 2. Accès SSH au Raspberry Pi (déploiement uniquement)
- Le Pi doit être joignable sur votre réseau avec SSH activé (`sudo raspi-config` > Interface Options > SSH).
- Notez son adresse IP.

### 3. sshpass (déploiement Linux / macOS uniquement)
`deploy.sh` utilise `sshpass` pour automatiser la saisie du mot de passe.
- **macOS** : `brew install hudochenkov/sshpass/sshpass`
- **Linux** : `sudo apt install sshpass`
- **Windows** : non requis — `deploy.ps1` utilise OpenSSH natif et demande le mot de passe interactivement (ou utilise une clé SSH).

### 4. Paramètres partagés par défaut
`defaults.sh` centralise les identifiants et chemins utilisés par les scripts de compilation, de déploiement et d'image (`AM_USER`, `AM_PASS`, ...). Il est **sourcé** par les autres scripts — modifiez-le une seule fois.

---

## 🛠 Compilation

### `build.sh [aarch64|armv7|all]`
Compilateur croisé intelligent. Compile le binaire Rust pour ARM 64 bits (`aarch64`, Pi 3/4/5 et Zero 2 W), ARM 32 bits (`armv7`, Pi 2/Zero) ou les deux, à l'intérieur de Docker.
Valeur par défaut : `all`.
**Utilisation** : `bash scripts/build.sh aarch64`

Le binaire produit est placé dans `target/<triple>/release/arcadematrix`.

---

## 🚀 Déploiement

Les scripts de déploiement détectent automatiquement l'architecture du Pi, compilent via Docker, transfèrent le binaire et redémarrent le service `arcadematrix`.

### `deploy.sh` (macOS / Linux)
Options (toutes facultatives ; les valeurs par défaut viennent de `defaults.sh`) :
- `--ip <IP>` — IP du Pi cible
- `--user <USER>` — Utilisateur SSH (par défaut `pi`)
- `--pass <PASS>` — Mot de passe SSH (utilise `sshpass`)
- `--skip-build` — Réutilise le binaire déjà compilé
- `--binary-path <PATH>` — Déploie un binaire précompilé spécifique

**Utilisation** : `bash scripts/deploy.sh --ip 192.168.1.149 --user pi`

### `deploy.ps1` (Windows)
Équivalent PowerShell utilisant OpenSSH natif. Même comportement.
**Utilisation** : `./scripts/deploy.ps1 -Ip 192.168.1.149 -User pi -Pass raspberry [-SkipBuild] [-BinaryPath <path>]`

> 💡 Pour un appareil déjà en service, vous n'avez généralement pas besoin de déployer via script : utilisez la **mise à jour OTA** dans l'interface Web (onglet Système), qui valide et remplace le binaire en Wi-Fi. Voir `docs/QUICKSTART_FR.md`.

---

## 💿 Création d'Image SD

### `build_image.sh`
Construit une image `.img` Raspberry Pi OS prête à flasher avec ArcadeMatrix préinstallé, via Docker. Produit une image d'environ 14 Go adaptée à une carte SD de 16 Go.
**Utilisation** : `bash scripts/build_image.sh`

Scripts internes (non appelés directement) :
- **`docker_builder.sh`** — s'exécute dans le conteneur Docker pour assembler l'image.
- **`chroot_setup.sh`** — s'exécute dans le chroot de l'image pour installer le système, le service et les dépendances via `autoInstall.sh`.

---

## 🩺 Exécution / Diagnostic sur le Pi

### `recovery.sh <PROJ_DIR>`
S'exécute sur le Pi **avant** le démarrage du service principal. Cherche sur la partition boot un firmware de secours ; s'il est présent, il l'installe et le sauvegarde pour éviter les boucles de redémarrage. Invoqué automatiquement par systemd.

### `wifi_diag.sh`
Enregistreur de diagnostic Wi-Fi / contention DMA sur le Pi. S'exécute en arrière-plan détaché (`setsid`), échantillonne périodiquement et écrit dans un fichier de log sur disque.
**Utilisation (sur le Pi, en root)** : `sudo bash scripts/wifi_diag.sh`

### `run_ab_test.sh`
Benchmark A/B automatisé (charge du bus mémoire vs hardware pulsing). S'exécute de manière totalement détachée (~6 min), survit aux coupures SSH et restaure la configuration initiale à la fin. Génère les fichiers `diag_*.log` utilisés dans `docs/benchmarks/`.

---

## ✅ Outillage & Validation

Ces scripts alimentent le hook git `pre-commit` et peuvent aussi être lancés manuellement.

### `install_hooks.py`
Installe les hooks git ArcadeMatrix dans `.git/hooks` (`pre-commit` exécute les validateurs + `cargo fmt --check` + tests).
**Utilisation** : `python3 scripts/install_hooks.py`

### `validate_docs.py`
Vérifie l'absence de dérive documentaire et la validité de `config.json` sur l'ensemble des documents EN/FR/ES.

### `validate_rpi_release.py`
Vérifie que les artefacts de release et scripts d'installation (`Cargo.toml`, `autoInstall.sh`, `config.json`, `README*.md`) sont présents et conformes avant une publication.

