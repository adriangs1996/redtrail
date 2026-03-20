---
title: Skills
description: Sistema de skills de Redtrail para extender funcionalidad.
---

:::caution[Próximamente]
Documentación completa con guías de desarrollo de skills y referencia de API se añadirá en una versión futura.
:::

Los skills son el mecanismo de extensión de Redtrail. Te permiten agregar nuevas capacidades, automatizar flujos de trabajo comunes y compartir técnicas con la comunidad.

## ¿Qué Son los Skills?

Un skill es un módulo autónomo que extiende la funcionalidad de Redtrail. Los skills pueden:

- Agregar nuevas integraciones de herramientas y parsers
- Definir flujos de trabajo de reconocimiento automatizado
- Proporcionar técnicas de enumeración especializadas
- Empaquetar patrones de explotación para escenarios comunes

## Skills Incluidos

Redtrail incluye un conjunto de skills integrados que cubren flujos de trabajo comunes de pentesting:

| Skill | Propósito |
|-------|-----------|
| `nmap-ingest` | Parsear e importar salida XML/grepable de nmap |
| `nikto-ingest` | Parsear resultados de escaneo de nikto |
| `gobuster-ingest` | Parsear resultados de fuerza bruta de directorios |
| `hydra-ingest` | Parsear resultados de fuerza bruta de credenciales |
| `enum-http` | Enumeración automatizada de servicios HTTP |
| `enum-smb` | Enumeración automatizada de servicios SMB |
| `enum-ftp` | Enumeración automatizada de servicios FTP |

*La lista de skills integrados está en evolución. Ejecuta `rt skill list` para el catálogo actual.*

## Gestión de Skills

```bash
# Listar skills instalados
rt skill list

# Instalar un skill de la comunidad
rt skill install <nombre>

# Eliminar un skill
rt skill remove <nombre>
```

## Desarrollo de Skills Personalizados

Puedes crear tus propios skills para automatizar tareas repetitivas o compartir técnicas:

```bash
# Crear estructura de un nuevo skill
rt skill create mi-skill
```

Los skills personalizados se definen como módulos estructurados con:

- **Metadatos** — nombre, versión, descripción, autor
- **Triggers** — cuándo se activa el skill (patrones de salida de herramientas, invocación manual)
- **Lógica** — qué hace el skill (parsear, extraer, enriquecer la KB)
- **Salida** — cómo se almacenan los resultados (entradas en KB, notas, hipótesis)

Guías detalladas de desarrollo y la referencia de API de skills se publicarán aquí una vez que el formato de skills esté estabilizado.
