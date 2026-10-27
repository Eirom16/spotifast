# Doremi: guía de agentes

`CONTRIBUTING.md`, `DOREMI-ROADMAP.md` y `F0-REPORT.md` son las referencias
del fork Eirom16/spotifast. Las antiguas restricciones de producto de
Spotifast no definen Doremi. Conserva su autoría, MIT y licencias de assets.

## Producto y seguridad

- Doremi será un reproductor nativo Rust, con egui/eframe, búsqueda y
  resolución mediante yt-dlp, sin Spotify obligatorio ni backend hospedado.
- Nada de Electron, motores de navegador embebidos, telemetría obligatoria,
  evasión de DRM, paywalls, restricciones geográficas o controles de acceso.
  Una negativa del proveedor es un error visible, no permiso para sortearla.
- No presentes el baseline Spotify de F0 como un reproductor Doremi terminado.
  No sustituyas silenciosamente una pista Spotify por una búsqueda de otro sitio.
- No borres una función sin inventariar consumidores, datos, tests y reemplazo
  en el roadmap. Las retiradas requieren una decisión explícita de alcance.
- No cambies `crmne/spotifast`, no publiques allí y no reutilices su identidad
  de firma, servicios de publicación o credenciales.

## Arquitectura

- Un único núcleo compartido por escritorio y Android. `doremi-core` no
  depende de egui, librespot, procesos, audio nativo ni APIs del SO.
- La UI dibuja y emite intenciones. Nunca bloquea con búsqueda, red,
  descargas, decodificación, keyring o esperas de disco. Usa workers,
  cancelación, deadlines, colas acotadas y eventos con generación.
- El callback de audio tampoco hace red, espera locks o asigna trabajo pesado.
- Los adaptadores encapsulan catálogo, resolución, salida de audio,
  persistencia y capacidades del SO. No copies lógica de negocio por plataforma.
- Mientras se extrae el legado, respeta `src/ui` → `Action` → `src/app.rs`
  → `src/backend.rs` y su runtime. No mezcles tipos Spotify con nuevos tipos
  neutrales. Consulta los tests próximos y `docs/_reference/queue.md` para
  preservar las reglas de cola.
- Los visualizadores reciben señal post-EQ/pre-volumen. Mantén esta propiedad.
- Escrituras atómicas y versionadas, migraciones reversibles y sin tocar
  perfiles reales en tests. Nunca registres tokens, cookies o URLs firmadas.

## Dependencias y contribuciones

- Prefiere crates.io. F0 congela las fuentes git heredadas de `crmne` tal como
  están en `Cargo.lock`; no implica propiedad de Doremi sobre esos repositorios.
  Cualquier fuente git nueva o cambiada necesita revisión explícita, revisión
  de licencia y pin de revisión. No copies fuentes de dependencias.
- No introduzcas ajustes, dependencias o rediseños ajenos a la fase.
  Explica cada cambio de interfaz y conserva evidencia fuera del repositorio.
- Trabaja en ramas de tema, con commits coherentes. No aterrices en una rama
  compartida, no hagas push ni reescribas historia sin autorización específica.
- Ejecuta los checks de `CONTRIBUTING.md`. No rebajes tests ni lints para
  ocultar un fallo. Distingue ejecutado, fallido, bloqueado y no ejecutado.
- Builds en `target/` del propio worktree. No uses `cargo clean` ni directorios
  compartidos para ahorrar espacio. No guardes capturas de revisión en el árbol.
- No publiques releases en F0. Una release futura necesita verificaciones del
  mismo commit y versión, checksums, licencias y pruebas nativas por plataforma.

## Documentación histórica

`docs/_guide/`, `docs/_reference/`, `PACKAGING.md`, recetas y assets describen
Spotifast cuando así lo indican. Son evidencia del baseline, no compromisos de
Doremi. Las automatizaciones originales se conservan desactivadas en
`.github/legacy-workflows/`. No las reactives sin revisar destinos, permisos,
secretos y gates. Lee `F0-REPORT.md` antes de migrar identidad persistida,
actualizaciones o empaquetado.
