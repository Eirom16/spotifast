# Workspace Doremi

F0 mantiene el paquete `spotifast` en la raíz, sin mover sus rutas ni tests.
Solo `doremi-core` existe como crate nuevo. No se crean crates vacíos para
simular módulos terminados. La versión se mantiene igual al paquete raíz;
`scripts/test-f0.py` rechaza divergencias hasta centralizar los manifests en F1.

Los módulos de catálogo, reproductor, biblioteca, UI y adaptadores se
introducirán con su primera extracción comprobada, siguiendo
`../DOREMI-ROADMAP.md`. Ningún paquete nuevo depende del legado.
