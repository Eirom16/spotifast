# Decisión: Navegador embebido para autenticación de YouTube Music

**Solicitante:** Eirom16 (propietario del repositorio)
**Fecha:** 2025
**Ámbito:** Solo autenticación OAuth/cookie de YouTube Music

## Decisión

Permitir el uso de un **navegador embebido** **EXCLUSIVAMENTE** para el flujo de inicio de sesión de YouTube Music, con las siguientes restricciones estrictas:

### ✅ Permitido
- Crear una ventana embebida **solo durante el proceso de autenticación**
- Usar esta ventana **únicamente** para el flujo OAuth de Google/YouTube Music
- Leer cookies de la sesión **solo desde esta ventana embebida**
- Cerrar y destruir completamente la ventana embebida **inmediatamente después de obtener las credenciales**

### ❌ Prohibido
- Usar el navegador embebido para cualquier otra función (navegación, reproducción, visualización de contenido)
- Mantener la ventana embebida abierta durante el uso normal de la aplicación
- Usar el navegador embebido para renderizar contenido de YouTube Music fuera del flujo de autenticación
- Cualquier uso que consuma recursos durante el uso cotidiano de Doremi

## Justificación (por Eirom16)

> "Propongo un navegador embebido. Pero quiero que solo se use para iniciar sesión y para nada más, ya que esto consume recurso. Lo digo porque no todo mundo tiene firefox en su computadora. Y claramente es un problema extraer las cookies desde un navegador cualquiera."

## Implementación

- Plataforma: Linux, Windows (usando el webview disponible en cada sistema)
- Android: Usar Android WebView (que ya es parte del runtime)
- El navegador embebido se crea **on-demand** solo cuando el usuario hace clic en "Iniciar sesión"
- Se destruye inmediatamente después de completar la autenticación

## Alternativa anterior (reemplazada)

Previamente se usaba navegador externo con:
1. Prioridad a Chrome/Edge/Chromium con lectura automática de cookies
2. Fallback a navegador por defecto del sistema
3. Pegado manual de cookies como último recurso

## Impacto

- ✅ Elimina la dependencia de navegadores externos
- ✅ Funciona en sistemas sin Firefox/Chromium instalado
- ✅ Experiencia de usuario más integrada
- ✅ No afecta el rendimiento durante el uso normal de la app

## Auditoria

Cualquier uso del navegador embebido fuera del flujo de autenticación debe ser considerado un error crítico y corregido inmediatamente.

---

**Registro:** Esta decisión supera la restricción general "Nada de Electron, motores de navegador embebidos" **exclusivamente** para el caso de uso de autenticación, con las condiciones estrictas mencionadas.
