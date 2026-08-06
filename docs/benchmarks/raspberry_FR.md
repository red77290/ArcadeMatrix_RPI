🇬🇧 [English](raspberry.md) | 🇫🇷 Français | 🇪🇸 [Español](raspberry_ES.md)

# Benchmarks Raspberry Pi

Ce document compare les performances d'ArcadeMatrix entre différentes implémentations de langages et modèles de Raspberry Pi.

## Comparaison des Langages (Pi 4, Matrice 64x64)

Le tableau suivant démontre pourquoi ArcadeMatrix a été migré de Python vers Rust, et comment il se compare à une implémentation théorique en pur C/C++.

| Métrique | Python (Legacy) | C / C++ (Théorique) | Rust (Actuel) |
| :--- | :--- | :--- | :--- |
| **FPS Max (Stable)** | ~45 FPS (saccades) | 100+ FPS (ultra stable) | **100+ FPS** (ultra stable) |
| **Usage CPU** | 35% - 50% | ~2% - 5% | **~2% - 5%** |
| **Empreinte RAM** | ~50 Mo | ~5 Mo | **~8 Mo** |
| **Jitter (Micro-ralentissements)** | Élevé (Garbage Collection) | Aucun (Mémoire Manuelle) | **Aucun** (Zero-Cost Abstractions) |
| **Sécurité** | Haute (Erreurs au runtime) | Faible (Risques de Segfault) | **Haute** (Sécurité mémoire à la compilation) |

### Pourquoi Rust ?
Comme le montrent les benchmarks, Rust offre exactement les mêmes performances "bare-metal" et la même prévisibilité de framerate que le C/C++ (éliminant les saccades liées au Garbage Collector de Python), tout en garantissant la sécurité de la mémoire et des threads. C'est crucial pour une architecture concurrente mêlant serveur web et rendu matériel.

---

## Performances Matérielles (Implémentation Rust)

Ces benchmarks représentent l'implémentation Rust actuelle (`arcadematrix`) sur différentes générations de Raspberry Pi.

### Pi Zero 2 W (Matrice 64x64)
- **Flip Clock Renderer**: ~100 FPS (Limité par le rafraîchissement des panneaux, pas par le CPU)
- **Cyberpunk Renderer**: ~100 FPS
- **GIF Engine**: ~60 FPS (Limité par la vitesse de lecture I/O de la carte SD)
- **Usage CPU**: ~10-15% 

### Pi 4 / Pi 5 (Matrice géante 256x64)
- **Flip Clock Renderer**: ~100 FPS 
- **Cyberpunk Renderer**: ~100 FPS
- **GIF Engine**: ~100+ FPS 
- **Usage CPU**: ~1-3% (Le Pi 4/5 sature facilement les limites du DMA GPIO bien avant de saturer le CPU)
