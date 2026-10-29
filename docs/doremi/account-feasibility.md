# Cuenta opcional: capacidades y límites

Revisión de fuentes primarias: 2026-10-29. Investigación posterior al primer
corte Android/audio; **no implementa login ni acredita acceso a una cuenta**.
No cambia Spotify legado, perfiles, credenciales o reproducción existente.

## Datos oficiales y colecciones musicales

| Capacidad | Evidencia y alcance |
|---|---|
| Canal propio | [`channels.list(mine=true)`](https://developers.google.com/youtube/data/reference/rest/v3/channels/list), con autorización del usuario |
| Playlists propias | [`playlists.list(mine=true)`](https://developers.google.com/youtube/data/reference/rest/v3/playlists/list), paginadas; no equivale a todas las colecciones guardadas en Music |
| Elementos de una playlist accesible | [`playlistItems.list`](https://developers.google.com/youtube/data/reference/rest/v3/playlistItems/list), respetando disponibilidad y privacidad |
| Vídeos valorados | [`videos.list(myRating=like)`](https://developers.google.com/youtube/data/reference/rest/v3/videos/list); la documentación actual limita este filtro a los 1000 más recientes. No prometer todos los «Me gusta» de Music |
| Álbumes, artistas y canciones guardados en Music, subidas musicales, radio/mixes e historial Music | No se identificaron endpoints públicos equivalentes en la referencia de YouTube Data API. Marcar capacidad no soportada; no sintetizarla a partir de playlists/canales |
| Audio nativo o URLs de medios | Data API proporciona recursos/metadatos, no un permiso general de extracción/reproducción de audio |

Las [políticas de YouTube API](https://developers.google.com/youtube/terms/developer-policies)
prohíben separar componentes audiovisuales, reproducción en un reproductor
oculto y scraping en los términos allí descritos. OAuth no elimina esos límites.
La arquitectura propuesta usa el catálogo oficial por separado y conserva medios
locales/CC0 de fuentes que permitan su reproducción nativa. No se conectará el
token oficial a un extractor para ampliar sus capacidades. La caché oficial debe
aplicar los plazos de actualización/borrado de III.E; desconexión y eliminación
de datos tendrán operaciones explícitas. No se copiarán datos personales a CI.

## Alternativas no oficiales

La documentación primaria de [ytmusicapi](https://ytmusicapi.readthedocs.io/en/stable/)
declara que emula solicitudes del cliente web y no tiene respaldo de Google.
Su [referencia de biblioteca](https://ytmusicapi.readthedocs.io/en/stable/reference/library.html)
describe canciones, álbumes y artistas guardados, diferenciando canciones de
biblioteca y canciones valoradas. Que un método exista no acredita acceso
permitido, estabilidad contractual ni derechos de reproducción.

Estas alternativas tienen esquemas privados cambiantes y riesgo de perder
acceso; una licencia del código no autoriza usar el servicio ni redistribuir
grabaciones. El mecanismo browser-auth requiere material de sesión y queda
excluido por decisión del producto. No se implementa una alternativa no oficial
autenticada en este corte, ni se prueba con cuentas/cookies reales. RustyPipe
sigue siendo referencia pendiente de licencia. No hay decisión de proveedor
YouTube basada en el experimento independiente de Commons.

## Autorización y retorno por plataforma

En Linux/Windows: cliente público Desktop, navegador del sistema, PKCE S256,
`state` aleatorio de un solo uso y listener restringido a loopback con puerto
efímero, ruta exacta y deadline. Se descartan retornos ajenos/expirados y se
cierra el listener tras completar/cancelar. La [guía oficial Desktop](https://developers.google.com/youtube/data/guides/auth/installed-apps)
indica que esos clientes no pueden guardar un secreto confidencial. Propone
`youtube.readonly` para el primer corte de lectura; no solicita permisos de
escritura, correo o perfil adicional sin una función que los necesite.

Android necesita una decisión específica: la guía anterior ya no admite custom
URI schemes para clientes Android y depreca loopback móvil. No se traslada el
redirect Desktop al teléfono. La [guía Android vigente](https://developer.android.com/identity/authorization)
separa autenticación de autorización y recomienda `AuthorizationClient` para
datos Google, con retorno mediante Activity result. Registra package/firma y
reobtiene tokens cortos mediante autorización; su acceso offline usa backend.
Como Doremi no requiere backend, el primer diseño Android debe limitarse a
sesiones cortas renovables por ese mecanismo. Custom Tabs con PKCE solo se
adoptará si el registro y redirect realmente están soportados por Google;
abrir una pestaña por sí solo no resuelve el retorno seguro.

## Fronteras y gates pendientes

- **Identidad:** estado desconectado/conectando/conectado/expirado/error;
  cancelar conserva catálogo local. Volver a Doremi tras autorizar y mostrar
  capacidades reales de la sesión, sin inferir Premium ni permiso de audio.
- **Credenciales:** adaptadores propios, Secret Service en Linux, almacén
  protegido de Windows y claves Android Keystore para datos que deban
  persistirse. Si el almacén no está disponible, sesión en memoria o error;
  nunca fallback a JSON plano. No reutilizar identidades/perfiles Spotifast.
- **Catálogo personal:** worker opcional, paginado y acotado, cancelación,
  deadline y generación. Cuenta, catálogo y pista tienen IDs distintos;
  desconocer una colección produce capacidad ausente, no resultados ficticios.
- **Reproducción:** únicamente ID neutral y recurso temporal autorizado;
  ningún token Google en cola, UI, logs, manifiestos, artefactos o resolutor público.
- **Cerrar sesión:** detener sincronización, invalidar generación, borrar
  credenciales y datos asociados acordados, solicitar revocación y mostrar si
  falta red; no modificar la cuenta Google ni borrar biblioteca local ajena.

Faltan registro de clientes/consentimiento del titular, revisión del flujo
Android, pruebas nativas del almacén y round trip real. Antes de implementar
se necesita aprobar este alcance concreto; no se pide intervención mientras
siguen disponibles los cortes independientes de audio/proveedores. El gate de
aceptación futuro exige datos reales soportados, cancelación/denegación,
retorno, expiración, revocación y cierre en cada plataforma obligatoria.

Referencia histórica inspeccionada, sin copiar implementación:
[`login_dialog.py`](https://github.com/Eirom16/Doremi-Rust/blob/1e44a21f883810a8337b31180ada9bf0fb0d4daf/src/doremi/ui/dialogs/login_dialog.py)
y [`youtube_music.py`](https://github.com/Eirom16/Doremi-Rust/blob/1e44a21f883810a8337b31180ada9bf0fb0d4daf/src/doremi/api/youtube_music.py).
Se preservan conectar/volver/estado/cerrar. Se excluyen WebEngine, interceptar
cookies, modificar Storage Access API y usar sesiones temporales exportadas.
