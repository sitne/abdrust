# abdrust

Una base para ejecutar un Discord Bot y una Actividad Embebida juntos en un único proyecto Rust. Construido para mantenimiento a largo plazo: la interfaz de la Actividad y la capa de voz del bot evolucionan en un mismo código, con un token, un repositorio.

## Qué Obtienes

- Desarrolla bot y Actividad en el mismo proyecto
- Comandos de barra `/voice join` / `/voice leave` / `/voice status`
- La Actividad muestra el estado del bot y eventos de voz en tiempo real
- Se ejecuta como una Aplicación Embebida de Discord

## Características Clave

- Bot, backend y Actividad en un solo repositorio Rust
- Recepción de voz, descifrado y diagnóstico canalizados directamente a la interfaz de la Actividad
- `/voice-diag` para verificación centralizada del estado de DAVE / unión / recepción
- Capa de voz abstracta — intercambia implementaciones sin tocar el resto

## Inicio Rápido

```bash
git clone <this-repo> abdrust
cd abdrust
cp .env.example .env
```

Edita `.env` con tus credenciales de Discord, luego inicia:

```bash
make dev
```

O ejecuta cada parte por separado:

```bash
cd backend && cargo run -p abdrust
cd frontend && npm run dev
```

## Configuración Requerida

| Variable | Descripción |
|---|---|
| `DISCORD_TOKEN` | Token del bot |
| `DISCORD_CLIENT_ID` | ID de cliente de la aplicación |
| `DISCORD_CLIENT_SECRET` | Secreto de cliente OAuth |
| `DISCORD_REDIRECT_URI` | URI de redirección OAuth |
| `DISCORD_GUILD_ID` | ID del servidor de desarrollo |
| `ACTIVITY_MODE` | `local` para desarrollo local, `discord` para producción |

## Ejecutando la Actividad

Debido al CSP de Discord, la Actividad requiere un túnel `cloudflared` para pruebas locales:

```bash
make tunnel
```

Configura la URL `https://*.trycloudflare.com` mostrada en el Discord Developer Portal bajo URL Mapping `/`. Lanza desde el Activity Shelf de Discord — no uses URL Override.

## Lista de Verificación

- backend: `GET /api/health` responde
- bot: conectado al Discord Gateway
- bot: `/abdrust-debug` responde
- voz: `/voice join` tiene éxito
- actividad: `initDiscord()` → `POST /api/token` → `ws` → `bot: ready` pasan correctamente

## Ejecutando Pruebas

```bash
# Backend
cargo test

# Frontend
npm run build

# Ambos
make check
```

## Herramientas del Navegador

Ver `docs/browser-tooling-playbook.md` para el flujo completo.

```bash
cd frontend && npm run test:e2e     # Playwright
cd frontend && npm run test:a11y    # Accesibilidad axe
cd frontend && npm run lighthouse   # Lighthouse CI
```

Estos se ejecutan en modo local `?tooling=1` — la autenticación de Discord, WS y APIs privadas están deshabilitados. Solo disponible en localhost.

## Manejo de `.env`

- Solo se usa el `.env` en la raíz
- El backend lee `../.env`
- El frontend lee el `.env` de la raíz a través de Vite al construir

## Comandos Adicionales

```bash
make check            # cargo check + npm run build
make cleanup-commands # limpiar comandos de barra registrados
```

## Estructura del Proyecto

```
abdrust/
├── backend/app/src/    # Backend Rust (bot, motor de voz, servidor HTTP)
├── frontend/src/       # UI de Actividad con React + TypeScript
├── docs/               # Arquitectura, ADRs, registro de desarrollo
├── scripts/            # Scripts de utilidad
├── AGENTS.md           # Instrucciones para agentes IA
└── .env                # Fuente única de verdad para configuración
```

## Principios de Diseño

- Preferir cambios fáciles de entender tras un clon fresco
- Mantener el código del bot y la Actividad alineado cuando el comportamiento cruza fronteras
- Optimizar para mantenibilidad futura, no solo el camino más corto para pasar el build
- Asumir que las APIs de Discord cambiarán — mantener la arquitectura flexible

## Licencia

- Código del repositorio y documentación original: MIT, ver `LICENSE`
- `protocol.md`: licencia CC BY-NC-SA 4.0 separada, ver `THIRD_PARTY_NOTICES.md`

## Idiomas

- [English](README.md)
- [日本語](README.ja.md)
- [中文](README.zh.md)
- [Español](README.es.md)
