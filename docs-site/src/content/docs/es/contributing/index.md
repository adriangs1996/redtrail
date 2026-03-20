---
title: Contribuir
description: Cómo contribuir al proyecto Redtrail.
---

:::caution[Próximamente]
Guías completas de contribución, documentación de estilo de código e instrucciones de configuración de desarrollo se añadirán en una versión futura.
:::

Redtrail es open source y da la bienvenida a contribuciones. Esta sección te guiará a través de la arquitectura del proyecto y el flujo de trabajo de desarrollo.

## Resumen de Arquitectura

Redtrail es una aplicación Rust organizada en varios módulos clave:

| Módulo | Propósito |
|--------|-----------|
| `tui/` | Interfaz de terminal — shell basado en bloques con paneles modales |
| `agent/` | Integración LLM — bucle del driver, proveedores, estratega |
| `knowledge.rs` | Base de conocimiento — hosts, puertos, creds, flags, notas |
| `attack_graph.rs` | Construcción y recorrido del grafo de ataque |
| `strategist.rs` | Protocolo deductivo BISCL (L0–L4) |
| `reactor.rs` | Procesamiento de eventos y transiciones de estado |
| `db.rs` | Capa de base de datos SQLite |
| `report/` | Generación de informes (Markdown, PDF) |
| `flags.rs` | Captura y seguimiento de flags |
| `types.rs` | Definiciones de tipos compartidos |
| `error.rs` | Tipos de error y manejo |

## Primeros Pasos

```bash
# Clonar el repositorio
git clone https://github.com/user/redtrail.git
cd redtrail

# Compilar
cargo build

# Ejecutar tests
cargo test

# Ejecutar con logging de depuración
RUST_LOG=debug cargo run
```

## Áreas de Contribución

- **Parsers de herramientas** — agregar soporte de importación para nuevas herramientas de pentesting
- **Skills** — crear y compartir módulos de skills reutilizables
- **Plantillas de informes** — formatos y estilos de salida adicionales
- **Documentación** — mejorar guías, tutoriales y documentación de API
- **Corrección de bugs** — revisar el rastreador de issues para bugs abiertos

Guías detalladas de contribución, proceso de PR y documentación de estilo de código se publicarán aquí conforme el proyecto madure.
