---
title: Conceptos Fundamentales
description: Conceptos fundamentales detrás de la metodología deductiva de pentesting de Redtrail.
---

:::caution[Próximamente]
Esta sección está en desarrollo activo. El contenido completo con ejemplos y diagramas se añadirá en una versión futura.
:::

Redtrail está construido alrededor de un enfoque estructurado y deductivo para las pruebas de penetración. Comprender estos conceptos fundamentales te ayudará a aprovechar al máximo la herramienta.

## Modelo de Workspace

Un **workspace** es un directorio aislado que contiene todo el estado de un engagement. Cuando ejecutas `rt init --target <ip>`, Redtrail crea una base de datos SQLite local, archivos de configuración y seguimiento de sesión en el directorio actual. Cada workspace es autónomo y portable.

## Base de Conocimiento (KB)

La base de conocimiento es el almacén central de datos de Redtrail. Recopila y organiza automáticamente:

- **Hosts** — objetivos descubiertos y sus metadatos
- **Puertos / Servicios** — puertos abiertos con identificación de servicio
- **Credenciales** — credenciales recolectadas o probadas
- **Flags** — artefactos de prueba de compromiso capturados
- **Notas** — observaciones en formato libre adjuntas a cualquier entidad

Los datos ingresan a la KB mediante la importación de salida de herramientas (`rt ingest`), entrada manual (`rt kb`), o captura automática de comandos proxiados.

## Hipótesis

Las hipótesis son el núcleo de la metodología de Redtrail. En lugar de pruebas ad-hoc, formulas teorías de ataque explícitas y las rastreas a través de su ciclo de vida:

1. **Propuesta** — una teoría inicial basada en datos observados
2. **En prueba** — siendo investigada activamente
3. **Confirmada** — respaldada por evidencia
4. **Refutada** — desmentida, registrada para el historial

Usa `rt hypothesis` (o `rt theory`) para crear, actualizar y revisar hipótesis durante un engagement.

## Capas Deductivas (L0–L4)

Redtrail organiza el flujo de trabajo de pentesting en cinco capas deductivas, cada una construyendo sobre la anterior:

| Capa | Nombre | Enfoque |
|------|--------|---------|
| **L0** | Reconocimiento | Recopilación de información pasiva y activa |
| **L1** | Enumeración | Sondeo e identificación a nivel de servicio |
| **L2** | Análisis de Vulnerabilidades | Mapeo de hallazgos a debilidades conocidas |
| **L3** | Explotación | Intento de confirmar vulnerabilidades |
| **L4** | Post-Explotación | Escalada de privilegios, movimiento lateral, exfiltración de datos |

El asesor LLM utiliza estas capas para sugerir próximos pasos apropiados a tu progreso actual.

## Framework BISCL

**BISCL** (Breadth-first, Iterative, Structured, Contextual, Layered) es el protocolo estratégico que guía al asesor de Redtrail:

- **Breadth-first** — enumerar toda la superficie de ataque antes de profundizar
- **Iterative** — revisitar capas anteriores cuando surge nueva información
- **Structured** — cada acción se vincula a una hipótesis
- **Contextual** — las sugerencias consideran el estado completo de la KB
- **Layered** — progresar a través de L0→L4 metódicamente

Juntos, estos conceptos aseguran que los engagements sean exhaustivos, reproducibles y bien documentados.
