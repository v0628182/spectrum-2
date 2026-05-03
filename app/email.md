# Welcome Email — Config & Deploy

## Lo que hice

### 1. Template de bienvenida

Diseñé un email en el mismo estilo ultraminimalista de la app y la web:

- **Fondo:** `#030303` (The Abyss)
- **Acento:** `#eeff00` (toxic yellow)
- **Tipografía:** JetBrains Mono + Inter
- **Tono:** directo, sin fluff, competitivo

El diseño vive en dos lugares:

| Archivo | Rol |
|---|---|
| `paraconectar/app4/welcome-email.html` | Vista previa visual (ábrelo en el navegador) |
| `.vps_api/server.js` (líneas 119-170) | Template real que envía Nodemailer (inline CSS puro) |

Ambos son idénticos visualmente. El de `server.js` es el que llega al inbox del usuario.

### 2. Infraestructura: cómo se dispara el email

Agregué un endpoint nuevo en el Express server y modifiqué todos los clients para que **cada vez que alguien se registre**, el welcome email se dispare automáticamente:

| Fuente de registro | Qué dispara el email |
|---|---|
| **API (`api.vanysound.com/api/auth/register`)** | El endpoint envía el welcome email directamente (ya existía, ahora con el nuevo template) |
| **App de escritorio (`authService.ts` → `signUp()`)** | Después de `supabase.auth.signUp()`, llama a `POST /api/auth/send-welcome` (fire-and-forget) |
| **Web (`login.html` → `handleSignup()`)** | Después de `supabaseClient.auth.signUp()`, llama a `POST /api/auth/send-welcome` (fire-and-forget) |

El endpoint `/api/auth/send-welcome`:
1. Recibe `{ email }` por POST
2. Busca `display_name` en `public.users` (la DB)
3. Si no encuentra, usa la parte antes del `@` del email como fallback
4. Envía el email con `nodemailer` (Hostinger SMTP)

### 3. Nombre del usuario en el email

**El email NUNCA muestra "operator".** En su lugar usa:

- `display_name` de la tabla `public.users` si existe
- Si no, `email.split('@')[0]` (ej: `gamer123@gmail.com` → `gamer123`)

El texto del email será: **"gamer123. Your account is ready..."**

### 4. Archivos modificados

| Archivo | Cambio |
|---|---|
| `.vps_api/server.js` | Nuevo template welcome + endpoint `POST /api/auth/send-welcome` |
| `paraconectar/app4/src/features/auth/authService.ts` | `signUp()` llama a la API para disparar el welcome email |
| `web/login.html` | `handleSignup()` llama a la API para disparar el welcome email |
| `paraconectar/app4/welcome-email.html` | Vista previa visual actualizada |

---

## Lo que necesitas hacer para deployar

### Paso 1: Subir `server.js` al VPS

```bash
scp .vps_api/server.js root@72.62.101.80:/opt/vanysound/.vps_api/server.js
```

### Paso 2: Reiniciar el proceso en el VPS

```bash
ssh root@72.62.101.80
pm2 restart vanysound-api
# o si usas otro nombre:
# pm2 list    (para ver los procesos)
# pm2 restart <nombre>
```

### Paso 3: Buildear la app de escritorio

```bash
cd paraconectar/app4
npm run tauri:build
```

Esto compila el `authService.ts` actualizado dentro del `.exe`.

### Paso 4: Desplegar la web

```bash
cd web
npm run build
npx wrangler pages deploy dist --project-name=vanysound-web
```

O si usas el método manual, sube `login.html` actualizado.

---

## Verificar que funciona

1. Regístrate con un email real desde la web o la app
2. Revisa el inbox (revisa spam también)
3. Deberías ver el email con tu `display_name` y el diseño nuevo

Si algo falla, los logs del VPS muestran errores: `ssh root@72.62.101.80 "pm2 logs vanysound-api --lines 20"`
