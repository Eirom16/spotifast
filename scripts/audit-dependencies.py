#!/usr/bin/env python3
"""Render/check the F0 inventory without resolving or downloading dependencies."""

import argparse
from collections import Counter
from pathlib import Path
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[1]
INVENTORY = Path("docs/doremi/dependencies.md")


def render(root):
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    lock = tomllib.loads((root / "Cargo.lock").read_text())
    lines = [
        "# Dependencias del workspace Doremi",
        "",
        "Generado con `python3 scripts/audit-dependencies.py`. No editar a mano.",
        "Clasificación y decisiones Android: `../../F0-REPORT.md`.",
        "Se conservan las fuentes git heredadas; los nuevos paquetes usan paths y crates.io.",
        "",
        "## Dependencias directas del workspace",
        "",
        "| Ámbito | Crate | Especificación |",
        "|---|---|---|",
    ]
    scopes = [(k, manifest[k]) for k in (
        "dependencies", "build-dependencies", "dev-dependencies"
    ) if k in manifest]
    for target, tables in sorted(manifest.get("target", {}).items()):
        for kind, deps in sorted(tables.items()):
            scopes.append((f"{target}: {kind}", deps))
    for member in manifest.get("workspace", {}).get("members", []):
        if member == ".":
            continue
        data = tomllib.loads((root / member / "Cargo.toml").read_text())
        for kind in ("dependencies", "build-dependencies", "dev-dependencies"):
            if kind in data:
                scopes.append((f"{member}: {kind}", data[kind]))
        for target, tables in sorted(data.get("target", {}).items()):
            for kind, deps in sorted(tables.items()):
                scopes.append((f"{member}/{target}: {kind}", deps))
    for scope, deps in scopes:
        for name, spec in sorted(deps.items()):
            if isinstance(spec, str):
                detail = spec
            else:
                detail = "; ".join(f"{k}={v}" for k, v in sorted(spec.items()))
            lines.append(f"| `{scope}` | `{name}` | `{detail}` |")
    sources = Counter(p.get("source", "workspace") for p in lock["package"])
    lines.extend([
        "", "## Fuentes transitivas bloqueadas", "",
        f"`Cargo.lock`: {len(lock['package'])} entradas (incluye miembros locales).",
        "", "| Paquetes | Fuente exacta |", "|---|---|",
    ])
    for source, count in sorted(sources.items()):
        lines.append(f"| {count} | `{source}` |")
    lines.extend([
        "", "## Patches", "",
        "| Crate | Fuente |", "|---|---|",
    ])
    for registry, patches in sorted(manifest.get("patch", {}).items()):
        for name, spec in sorted(patches.items()):
            lines.append(f"| `{registry}/{name}` | `{spec['git']} @ {spec['rev']}` |")
    lines.extend([
        "", "## Núcleo nuevo", "",
        "`crates/doremi-core` no tiene dependencias, build script ni código de SO.",
        "La resolución global del workspace aún incluye los patches del legado;",
        "compilar solo el núcleo no demuestra que el resto sea portable.", "",
    ])
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    expected = render(ROOT)
    if args.check:
        if not (ROOT / INVENTORY).is_file() or (ROOT / INVENTORY).read_text() != expected:
            print(f"{INVENTORY} is stale; render and review the inventory.", file=sys.stderr)
            return 1
        print("Dependency inventory matches the manifests and lockfile.")
    else:
        print(expected, end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())
