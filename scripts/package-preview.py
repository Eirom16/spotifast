#!/usr/bin/env python3
"""Package the already-tested Linux binary. Never builds or publishes a release."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tarfile

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    identity = json.loads((args.input / "manifest.json").read_text())
    sha = subprocess.check_output(["git", "rev-parse", "HEAD"], text=True, cwd=ROOT).strip()
    assert identity["source_commit"] == sha and not identity["source_dirty"]
    assert identity["kind"] == "doremi-preview" and not identity["release"]
    assert identity["target"] == "x86_64-unknown-linux-gnu"
    binary = args.input / "doremi-preview"
    assert digest(binary) == identity["payload"]["sha256"]
    version = re.search(r'pub const VERSION: &str = "([^"]+)";', (ROOT / "src/preview.rs").read_text())[1]
    args.output.mkdir(parents=True, exist_ok=True)
    folder = args.output / f"DoremiPreview-{version}"
    folder.mkdir()  # Refuse stale/mixed packages; no recursive delete.
    shutil.copy2(binary, folder / binary.name)
    (folder / binary.name).chmod(0o755)
    shutil.copy2(ROOT / "packaging/preview/run.sh", folder / "run.sh")
    (folder / "run.sh").chmod(0o755)
    for name in ["LICENSE", "Cargo.lock"]:
        shutil.copy2(ROOT / name, folder / name)
    shutil.copy2(ROOT / "packaging/preview/README.md", folder / "README.md")
    for name in ["doremi", "fonts", "icons"]:
        shutil.copytree(ROOT / "assets" / name, folder / "assets" / name)
    runtime = subprocess.run(["ldd", str(folder / binary.name)], capture_output=True, text=True, check=True)
    assert "not found" not in runtime.stdout, runtime.stdout
    (folder / "runtime-libraries.txt").write_text(runtime.stdout)
    (folder / "manifest.json").write_text(json.dumps({
        **identity, "legacy_version": identity["version"], "version": version,
        "product": "Doremi Preview", "phase": "Preview 0.1",
        "features": ["demo", "milkdrop"], "audio": False,
    }, indent=2) + "\n")
    files = sorted(p for p in folder.rglob("*") if p.is_file())
    (folder / "SHA256SUMS").write_text("".join(f"{digest(p)}  {p.relative_to(folder)}\n" for p in files))
    archive = args.output / f"doremi-preview-{version}-linux-x86_64.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        tar.add(folder, arcname=folder.name)
    (args.output / "SHA256SUMS").write_text(f"{digest(archive)}  {archive.name}\n")
    print(json.dumps({"archive": str(archive), "sha256": digest(archive), "source_commit": sha, "version": version}))


if __name__ == "__main__":
    main()
