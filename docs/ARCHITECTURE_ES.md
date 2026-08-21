🇬🇧 [English](ARCHITECTURE.md) | 🇫🇷 [Français](ARCHITECTURE_FR.md) | 🇪🇸 Español

# Visión General de la Arquitectura (Raspberry Pi - Rust)

Este documento proporciona una visión general completa de la arquitectura de ArcadeMatrix para Raspberry Pi en **Rust**.

---

## 1. Filosofía Principal

ArcadeMatrix gestiona una matriz LED HUB75 utilizando los bindings Rust de `rpi-rgb-led-matrix`.

---

## 2. El Pipeline de Renderizado

```mermaid
graph TD
    subgraph Capa de Datos y API
        API[Actix-web REST API]
        Config[config.json / ConfigLoader]
        Time[Sistema de Tiempo]
        Network[APIs Weather / MQTT / Crypto / Stock]
    end

    subgraph Capa de Motores (Rust)
        App[ArcadeMatrixApp]
        Rot[RotationState]
        ClockE[ClockEngine]
        DateE[DateEngine]
        WeathE[WeatherEngine]
        CryptoE[CryptoEngine]
        StockE[StockEngine]
        Rot --> ClockE & DateE & WeathE & CryptoE & StockE
    end

    subgraph Capa Estética
        ClockE --> Renderers[Renderers: Cyberpunk, Flip, TrueMatrix]
        Renderers --> Pil[Canvas image-rs]
    end

    subgraph Capa de Hardware
        Pil --> Wrapper[HardwareMatrix / MockMatrix]
        Wrapper --> Hardware[Matriz LED HUB75]
    end
```

---

## 3. Modelo de Hilos e Aislamiento de Runtime

- **Hilo de Renderizado Dedicado (`matrix-render`):** Hilo OS con stack de 8MB.
- **Hilo Web API Aislado (`api-server`):** Runtime Tokio mono-hilo para evitar interferencias de red con el driver de matriz.
- **Transmisión de Símbolos:** Validación de símbolos (Crypto / Stock) con omitido automático si no hay elementos válidos.
