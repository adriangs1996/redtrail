---
title: Referencia CLI
description: Referencia de todos los comandos CLI de Redtrail.
---

:::caution[Próximamente]
Uso detallado, flags y ejemplos para cada comando se añadirán en una versión futura. Ejecuta `rt <comando> --help` para detalles actuales.
:::

Redtrail se invoca mediante el comando `rt`. Cualquier comando no reconocido se proxia a tu shell con captura automática de salida.

## Comandos

| Comando | Alias | Descripción |
|---------|-------|-------------|
| `rt init` | | Inicializar un nuevo workspace en el directorio actual |
| `rt kb` | | Consultar y gestionar la base de conocimiento (hosts, puertos, creds, flags, notas) |
| `rt status` | `st` | Mostrar métricas de sesión: hosts, puertos, creds, flags, hipótesis |
| `rt hypothesis` | `theory` | Rastrear y gestionar hipótesis de ataque |
| `rt evidence` | `ev` | Registrar y gestionar evidencia y hallazgos |
| `rt session` | `sess` | Gestionar sesiones del workspace |
| `rt scope` | | Verificar si una IP está dentro del alcance definido |
| `rt config` | `conf` | Ver y modificar configuración (global y workspace) |
| `rt setup` | | Ejecutar el asistente de configuración interactivo o gestionar alias de herramientas |
| `rt ingest` | `eat` | Importar archivos de salida de herramientas a la base de conocimiento |
| `rt report` | `rep` | Generar un informe de prueba de penetración desde datos de sesión |
| `rt pipeline` | | Gestión de pipelines (diferido a v2) |
| `rt env` | | Imprimir comandos shell para activar el entorno redtrail |
| `rt deactivate` | `deact` | Imprimir comandos shell para desactivar el entorno redtrail |
| `rt skill` | | Gestionar skills de redtrail (crear, probar, instalar, eliminar) |
| `rt ask` | | Consultar al LLM con contexto completo de sesión e historial de conversación |
| `rt query` | `q` | Consulta LLM única con contexto de sesión (sin historial) |
| `rt sql` | | Ejecutar SQL contra la base de datos de redtrail |
| `rt help` | | Imprimir ayuda o la ayuda de un subcomando dado |

## Proxy de Shell

Cualquier comando no listado arriba se proxia automáticamente a tu shell y se registra en la sesión. Usa `rt -- <cmd>` para forzar el modo proxy en comandos que comparten nombre con un subcomando.

## Referencia Rápida

```bash
# Iniciar un nuevo engagement
rt init --target 10.10.10.1
eval "$(rt env)"

# Ejecutar herramientas (captura automática)
nmap -sV 10.10.10.1

# Importar resultados
rt ingest nmap.xml

# Rastrear hipótesis
rt theory add "SSH permite autenticación por contraseña"

# Consultar al asesor
rt ask "¿Qué debería intentar a continuación?"

# Generar informe
rt report generate
```
