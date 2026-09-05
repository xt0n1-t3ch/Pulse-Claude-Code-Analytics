[Documentación](../index.md) / OpenCode y Astra

# ![](../../assets/icons/terminal.svg) Usar OpenCode y GPT-6 Astra en Pulse

Esta referencia explica a usuarios y mantenedores de Pulse 1.8.1 cómo separar sesiones, cuentas y publicación en Discord. OpenCode funciona con datos locales; Astra usa el catálogo compartido con el runtime independiente; el pin del core y la copia del catálogo tienen contratos distintos. Los artefactos y sus versiones siguen el contrato de release de cada repositorio.

## Índice

- [Seleccionar sesiones y publicación](#seleccionar-sesiones-y-publicación)
- [Configurar OpenCode](#configurar-opencode)
- [Interpretar los datos](#interpretar-los-datos)
- [Verificar Astra y las imágenes](#verificar-astra-y-las-imágenes)
- [Actualizar y recuperar](#actualizar-y-recuperar)
- [Detección del plan y override](#detección-del-plan-y-override)
- [Límites OpenCode Go](#límites-opencode-go)
- [Perfil Discord local](#perfil-discord-local)
- [Notificaciones y eficiencia](#notificaciones-y-eficiencia)

## Seleccionar sesiones y publicación

Una sola barra selecciona Claude, Codex, OpenCode o All providers. Elegir una aplicación selecciona su contexto y su historial; All providers agrega analytics sin cambiar el último broadcaster elegido. Las cuotas conservan su proveedor y autenticación propios, aunque compartan el selector visual.

OpenCode Desktop, CLI y OpenChamber comparten el mismo nombre y logo OpenCode en Discord. La lectura de SQLite no requiere un plugin dentro del agente. Pulse conserva una superficie genérica cuando no puede atribuir una sesión a un cliente concreto. Un registro de OpenChamber solo aporta evidencia si coinciden el PID, el proceso padre, el ejecutable y la asociación de sesión, sin otro backend concurrente.

La interfaz mantiene el sistema visual de Pulse en las seis vistas. Las cuotas se distribuyen según el ancho disponible, sin una columna recortada ni altura fija. En móvil, la navegación usa dos filas completas, los proveedores usan un selector compacto y Home permite desplegar sus cuotas para priorizar la sesión. Sessions y Costs usan métricas en dos columnas. Las tablas mantienen scroll horizontal explícito y acceso por teclado; cambiar de vista restablece la posición de lectura.

Los controles conservan navegación por teclado, estado seleccionado y foco visible. La pestaña activa usa el patrón neutro de los selectores: fondo suave, borde visible y texto de alto contraste. Los encabezados móviles comparten alineación central. Las métricas Home ocupan columnas iguales; el heatmap usa una fila propia. El preview muestra las dos líneas calculadas por Rust. El texto se ajusta dentro de la tarjeta, sin duplicar el payload en otro panel. Una imagen pequeña no disponible no se reemplaza por el logo Pulse.

## Configurar OpenCode

Pulse busca `opencode.db` y los archivos de canal `opencode-*.db` dentro del directorio de datos OpenCode. Respeta `XDG_DATA_HOME` y `OPENCODE_DB`. Una ruta relativa de `OPENCODE_DB` se resuelve dentro del directorio de datos. `:memory:` no es una base compartida y se ignora.

La configuración propia de Pulse vive en `~/.claude/pulse-opencode.json`, o dentro de `CLAUDE_HOME` cuando está configurado. No modifica las credenciales ni la configuración de OpenCode. `database_paths` acepta archivos SQLite adicionales; `enabled`, `privacy_enabled`, `client_id` y `layout` controlan la publicación. Las rutas de base de datos duplicadas se normalizan antes de leerlas.

El Application ID predeterminado es `1545590419763761303`; la clave de imagen es `opencode-v2`. Los datos remotos y servidores OpenChamber alojados en otras máquinas quedan fuera de esta integración local.

## Interpretar los datos

El lector abre SQLite en modo read-only y reconoce tablas `message` o `session_message`. Importa lotes de 64 sesiones mediante un cursor estable y revisita sesiones recientes. Solo confirma el cursor cuando se completa la persistencia. Las sesiones recientes sin cambios reutilizan sus metadatos calculados.

Pulse conserva proveedor del modelo, ID original, nombre visible, variante, base de origen y contribuciones por modelo. Una sesión completada deja de ser elegible para la presencia y el foco activo; sus datos permanecen en History. La respuesta más reciente identifica el modelo utilizado; la selección de sesión sirve como fallback antes de la primera respuesta. Una sesión multimodelo aparece como Mixed models, con su desglose, sin atribuir todo el consumo al último modelo.

Un coste cero reportado por OpenCode no equivale a un coste desconocido. OpenCode-reported value expresa el valor registrado por la aplicación, no una factura confirmada del proveedor. Pulse no reconstruye precios para modelos OpenCode sin un coste reportado. Tampoco inventa cuotas de cuenta ni capacidad de contexto.

El contexto proviene del mensaje o de los metadatos configurados para el par proveedor/modelo. El acumulado de tokens de sesión no representa el contexto ocupado. Las recomendaciones específicas de Claude permanecen desactivadas para OpenCode.

Los diagnósticos del lector aparecen en Settings. Los prompts y argumentos de comandos no entran en el payload de Discord. `privacy_enabled` omite proyecto y rama; los demás campos siguen su configuración explícita.

## Verificar Astra y las imágenes

`gpt-6-astra` se muestra como GPT-6 Astra. El [catálogo de modelos](../models/codex.md) es la referencia única para ventanas, esfuerzos, tarifas y fuentes verificadas.

> La API de Astra expone 1.050.000 tokens totales, hasta 922.000 de entrada y 128.000 de salida. El inventario local Codex 0.153.3 del 2026-09-05 expone 272.000 brutos y 258.400 usables. No son la misma capacidad ni una garantía para todas las cuentas.

`ultra` se conserva como valor observado del harness, no como nivel publicado de la API. Los límites y costes de OpenCode no se heredan de Astra por compartir esta página. Consulte también las [diferencias del catálogo incluido](../models/codex.md#bundled-catalog-gaps).

El catálogo canónico vive en el repo Codex-Discord-Rich-Presence. `scripts/check-model-catalog-parity.ps1` comprueba la igualdad byte a byte con Pulse. El pin del core 2.0.0 sigue intacto; el cambio local no se atribuye al commit remoto fijado.

`assets/branding/opencode.provenance.json` registra la imagen proporcionada por Tony y sus checksums, junto con la fuente anterior para rollback. El mismo PNG se incluye en el preview y se sube al Developer Portal. La presencia de la clave en el portal o un payload correcto no sustituyen la verificación visual en Discord.

## Actualizar y recuperar

Antes de sustituir Pulse, respalda el ejecutable, las configuraciones y la base con SQLite Backup. No copies una base activa sin sus transacciones. La migración a esquema 6 añade `opencode_json` y conserva las filas anteriores; el mecanismo existente crea además un respaldo previo a la migración.

Cierra únicamente el PID cuyo ejecutable coincide con la instalación Pulse. Instala el binario verificado y compara su SHA-256. Comprueba arranque, historial y `PRAGMA quick_check`. Conserva Discord, OpenCodex y los agentes.

Para rollback, cierra esa misma instalación, conserva la base nueva y restaura el binario, las configuraciones y la copia SQLite consistente. Las sesiones importadas después del respaldo se conservan en la base apartada, no en la restaurada.

## Detección del plan y override

El override manual de Settings tiene prioridad sobre la telemetría, la memoria y la caché. Auto-detect consulta `account/read` junto a `account/rateLimits/read` en el mismo proceso autenticado. Los datos del plan no crean ventanas de cuota ficticias.

Pulse reconoce Free, Go, Plus, Pro 5x, Pro 20x, Business, Enterprise y Edu. El valor de protocolo `pro` identifica Pro 20x; `prolite` identifica Pro 5x. También acepta los aliases explícitos `pro_5x` y `pro_20x`. Los planes Claude Max no se interpretan como planes Codex. Una señal no reconocida no reemplaza una identificación válida.

La matriz de pruebas cambia cada override, vuelve a cargar su archivo y comprueba el mismo plan en Settings y en el payload de Discord. Volver a Auto-detect utiliza las señales de cuenta, no el último override. Un override inválido falla sin cambiar la configuración.

## Límites OpenCode Go

El adaptador consulta `https://opencode.ai/zen/go/v1/usage` con la clave existente de `opencode-go` en el archivo de autenticación OpenCode. No copia la clave a Pulse ni a los logs. El worker consulta cada 60 segundos con timeout y sin redirecciones. Solo una respuesta válida habilita las ventanas de 5 horas, semanal y mensual.

El API proporciona porcentajes y fechas de reinicio. La ventana mensual conserva su etiqueta y fecha real; no se inventa una duración fija de 30 días. Los límites pertenecen a la suscripción Go, no a todos los modelos o proveedores usados desde OpenCode.

Usage quotas controla las tres ventanas en Discord y en el preview. Los presets y el orden guardado usan el mismo compositor. El texto incluye Go y used para identificar la cuenta, incluso cuando el modelo activo pertenece a otro proveedor. Los datos caducados o incompletos no se publican. Sin una sesión reciente, OpenCode retira su actividad de Discord y el preview muestra Waiting for OpenCode. El layout predeterminado empieza por modelo, actividad, proyecto y rama.

Home filtra el resumen, historial y actividad por proveedor y últimos siete días. La proyección mensual conserva la etiqueta This month. El modelo de la sesión seleccionada reemplaza el modelo histórico mientras existe una sesión reciente.

## Perfil Discord local

La identidad de `READY` del IPC local posee el usuario, nombre visible y avatar. La consulta no solicita credenciales ni publica una actividad. El banner solo se usa si el cliente lo expone o existe una referencia de imagen del mismo usuario en la caché local. Si falta, Pulse no dibuja un banner simulado. En la validación de esta instalación, `READY` y `GET_USER` no incluyeron banner.

## Notificaciones y eficiencia

El centro permite marcar avisos como leídos o no leídos, aplicar esas acciones a todos y limpiar la lista con confirmación. La limpieza conserva los registros; Undo restaura el último lote confirmado y sus estados de lectura. Los fallos de transporte conservan la última lista y no afirman que una mutación se guardó.

[Windows Efficiency mode](../maintainers/windows-efficiency.md) describe EcoQoS, la prioridad reducida y el opt-out.
