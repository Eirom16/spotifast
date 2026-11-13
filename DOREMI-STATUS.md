# Doremi — estado actual

- Rama: `feature/doremi-product-ui`; núcleo y UI compartidos Linux/Windows/Android.
- Catálogo principal YouTube Music nativo, adaptado de Tubefast MIT: búsqueda,
  artistas, álbumes, playlists e inicio con secciones reales. MusicBrainz opcional.
  Sin equivalencias ocultas de títulos o IDs entre proveedores.
- CI `43334288394`, commit `e31f4c7`: APK y hosts Doremi Linux/Windows aprobados,
  incluidos auxiliares, licencias, versión y checksums. Android genera los bindings
  QuickJS con el NDK en ambas ABI. Clippy general aprobado en `43334288569`;
  última CI general completa en verde: `43271026631` (`d605179`).
- APK `e31f4c7` instalada sin borrar datos ni sesión en Samsung ARMv7: aislamiento y cancelación
  del login comprobados. Inicio con 30 elementos, carátulas y buscador superior con
  teclado comprobados en `4f4e49e`, en ambos temas. Biblioteca conserva su hash.
  Navegación Linux real: 20 canciones, artista con 59 elementos, álbum con 18 pistas
  y playlist con 84. Se conservan IDs del proveedor y la navegación atrás.
- Sesión: Android usa WebView temporal en proceso separado y almacén Keystore;
  escritorio usa `doremi-auth` temporal, sin enlazar el motor web al host musical.
  Cancelar destruye proceso y perfil: comprobado en CachyOS y Samsung. Keystore
  sintético comprobado tras reinicio, con integridad y borrado. Cookies redactadas.
  Importación explícita de sesión autorizada de Doremi-Rust disponible.
- Login real Android validado en `557691f`: cuenta musical, regreso automático y
  restauración tras reiniciar el proceso. Mismo cliente para inicio personalizado
  (20 elementos), 11 playlists, primera página de 100 favoritas, 2 álbumes y
  9 artistas. Biblioteca local conserva su hash; sesión cifrada con Keystore.
  La causa era el bloqueo de `accounts.google.com.do` después de la verificación.
  Dominios regionales oficiales corregidos también en escritorio: cinco pruebas
  del auxiliar y paridad Android/escritorio aprobadas. Login real en CachyOS
  validado en `02aefab`: sesión restaurada al reiniciar y las mismas cinco páginas
  personales cargadas; auxiliar retirado y biblioteca local conservada. Login
  Windows pendiente de prueba real. Comparación con Meld/Metrolist GPL-3.0 adaptada con
  código propio, conservando licencias y sin registrar credenciales.
- CachyOS: bundle `e31f4c7` instalado con acceso directo y auxiliar verificados;
  versiones anteriores y perfiles conservados. Cancelación y retiro de los
  procesos del navegador comprobados desde la instalación.
  Carátula real del álbum de 18 pistas comprobada en claro y oscuro en `f67b410`.
- Windows: ejecutables nativos compilados. Arranque gráfico/audio bloqueados en
  runner por falta de OpenGL 2.0 y dispositivo de audio; falta prueba física.
  Tres pruebas sintéticas nativas aprobadas en Windows, incluidas compatibilidad
  UTF-16 y sesiones de 12 KiB fragmentadas en Credential Manager. La sesión real
  de Android excedía el límite de una entrada Windows; corregido en `02aefab`.
- Audio YouTube: rangos AAC/MP4, cancelación y seek conectados al motor existente.
  Misma sesión WEB_REMIX de catálogo, comprobación de caducidad y sin fallback
  tras una negativa. `d605179` corrigió UNPLAYABLE: prueba real Android devuelve
  OK y un AAC sin DRM que requiere firma de URL. `e31f4c7` supera la resolución,
  pero la apertura falla con InvalidMp4 en Samsung y CachyOS; todavía sin PCM.
  Firma y parámetro n integrados con preprocesado MIT y QuickJS acotado en el
  cliente compartido: pruebas offline y Clippy aprobados; firma y n comprobados
  con dos versiones públicas del reproductor. Nonce compartido entre petición y
  recurso, transporte CDN acotado y distinción entre fallo de entrada/decodificación
  integrados; prueba real del ajuste pendiente.
  Sesión y biblioteca conservadas. Invitado carga inicio; reproducción exige login por verificación
  antibot del proveedor desde la conexión del Samsung. No se reintenta otro cliente.
- Biblioteca, playlists propias, cola e historial conservados. CI gráfica Linux
  verifica navegación, teclado, duplicados, playlists y restauración.
- Evidencia: `~/.local/share/doremi/product-qa/2026-11-12/` y jornadas anteriores.
- Sin releases. Documentación histórica conservada.
