---
title: Configuración
description: Configurar los ajustes globales y de workspace de Redtrail.
---

:::caution[Próximamente]
Referencia completa de configuración con todas las opciones disponibles se añadirá en una versión futura.
:::

Redtrail utiliza un sistema de configuración por capas. Los ajustes del workspace sobreescriben los valores globales por defecto, dándote control por engagement.

## Configuración Global

Los ajustes globales aplican a todos los workspaces y se almacenan en `~/.config/redtrail/config.toml`. Configúralos con:

```bash
rt config set <clave> <valor>
```

La configuración global cubre:

- **Proveedor LLM por defecto** — qué backend de IA usar
- **Claves API** — credenciales para servicios LLM
- **Reglas de alcance por defecto** — rangos de IP para siempre incluir/excluir
- **Plantillas de informes** — formato y marca de salida por defecto
- **Integración con shell** — personalización del prompt, comportamiento de captura automática

## Configuración de Workspace

Cada workspace tiene su propia configuración almacenada en el directorio del workspace. Los ajustes del workspace sobreescriben los valores globales para ese engagement:

```bash
# Dentro de un workspace
rt config set target.ip 10.10.10.1
rt config set scope.networks "10.10.10.0/24"
```

Las opciones específicas del workspace incluyen:

- **Definición del objetivo** — IP, hostname, límites de alcance
- **Ajustes de sesión** — intervalo de auto-guardado, profundidad de historial
- **Reglas de importación** — qué salidas de herramientas parsear automáticamente
- **Metadatos del informe** — nombre del cliente, ID del engagement, fechas

## Proveedores LLM

Redtrail soporta múltiples backends LLM para su funcionalidad de asesor:

| Proveedor | Descripción |
|-----------|-------------|
| **Anthropic API** | Acceso directo a modelos Claude mediante clave API |
| **Ollama** | Inferencia local con modelos de pesos abiertos |

Configura el proveedor activo:

```bash
# Usar Anthropic
rt config set llm.provider anthropic
rt config set llm.anthropic.api_key sk-ant-...

# Usar Ollama local
rt config set llm.provider ollama
rt config set llm.ollama.model llama3
rt config set llm.ollama.url http://localhost:11434
```

Ejecuta `rt setup` para un asistente de configuración interactivo que te guía a través de todas las opciones.
