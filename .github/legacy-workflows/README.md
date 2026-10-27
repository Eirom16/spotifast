# Automatización histórica, desactivada

Estos archivos se movieron desde `.github/workflows/` y `.github/triage.yml`,
añadiendo solo un comentario de cuarentena. GitHub no ejecuta workflows aquí.
Se preservan como referencia de empaquetado y release del baseline. La CI
original se conserva en Git: `git show cd40da0:.github/workflows/ci.yml`.

Motivo: publicaban Spotifast, usaban dominios, repositorios, recetas y servicios
del upstream, o aplicaban políticas incompatibles con Doremi. No se autoriza
su ejecución, publicación ni acceso a secretos por conservar sus archivos.
La única CI activa es `../workflows/ci.yml`, de solo lectura del repositorio.

Antes de recuperar un job, revisar permisos, acciones externas, eventos
`pull_request_target`, scripts ejecutados, destinos, firmas y procedencia
de sus artefactos. El roadmap define las puertas de publicación futuras.
