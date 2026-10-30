#!/usr/bin/env python3
"""Install a verified development artifact privately, retaining older builds.

No root, releases, updater, Spotifast profile changes or removal of old versions.
Only our own managed desktop entry may be replaced.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact", type=Path, help="native-dist directory")
    parser.add_argument("--commit", required=True)
    args = parser.parse_args()
    assert re.fullmatch("[a-f0-9]{40}", args.commit)
    source = args.artifact.resolve()
    manifest = json.loads((source / "manifest.json").read_text())
    assert manifest["source_commit"] == args.commit and manifest["product"] == "Doremi"
    assert not manifest["release"] and not manifest["source_dirty"]
    assert manifest["target"] == "x86_64-unknown-linux-gnu"
    listed = set()
    for line in (source / "SHA256SUMS").read_text().splitlines():
        digest, name = line.split("  ", 1)
        path = (source / name).resolve()
        assert path.is_relative_to(source) and not (source / name).is_symlink()
        assert hashlib.sha256(path.read_bytes()).hexdigest() == digest, name
        listed.add(path)
    entries = list(source.rglob("*"))
    assert not any(path.is_symlink() for path in entries)
    assert {path.resolve() for path in entries if path.is_file()} == listed | {(source / "SHA256SUMS").resolve()}
    base = Path.home() / ".local/share/doremi/native-dev-builds"
    destination = base / args.commit
    assert not base.is_symlink()
    if destination.exists():
        assert not destination.is_symlink()
        assert json.loads((destination / "manifest.json").read_text()) == manifest
    else:
        base.mkdir(parents=True, exist_ok=True)
        shutil.copytree(source, destination)
    binary = destination / "doremi-app"
    assert not binary.is_symlink()
    assert hashlib.sha256(binary.read_bytes()).hexdigest() == manifest["payload"]["sha256"]
    binary.chmod(binary.stat().st_mode | 0o100)
    version = subprocess.check_output([str(binary), "--version"], text=True).strip()
    assert version == "Doremi Native Dev " + manifest["version"]
    icon = destination / "Doremi.png"
    expected = subprocess.check_output(["git", "show", args.commit + ":assets/doremi/Doremi.png"], cwd=ROOT)
    if icon.exists():
        assert not icon.is_symlink() and icon.read_bytes() == expected
    else:
        icon.write_bytes(expected)
    # This task's private paths do not require Desktop Entry escape processing.
    assert not any(char in str(binary) + str(icon) for char in '\n\r"$`\\')
    applications = Path.home() / ".local/share/applications"
    applications.mkdir(parents=True, exist_ok=True)
    entry = applications / "dev.eirom.doremi.native-dev.desktop"
    if entry.is_symlink() or (entry.exists() and "X-Doremi-Managed=true" not in entry.read_text()):
        entry = applications / ("dev.eirom.doremi.native-dev-" + args.commit[:7] + ".desktop")
        assert not entry.exists()
    text = f'''[Desktop Entry]
Type=Application
Name=Doremi Native Dev
Comment=Audio local experimental y catálogo de demostración offline
Exec="{binary}"
TryExec={binary}
Icon={icon}
Terminal=false
Categories=AudioVideo;Audio;Player;
X-Doremi-Managed=true
X-Doremi-SourceCommit={args.commit}
'''
    temporary = entry.with_suffix(".desktop.tmp")
    assert not temporary.exists() and not temporary.is_symlink(), "Preserve an unexpected temporary entry"
    temporary.write_text(text)
    os.replace(temporary, entry)
    result = dict(source_commit=args.commit, binary=str(binary),
                  sha256=manifest["payload"]["sha256"], desktop_entry=str(entry),
                  data_preserved=True, version=version)
    output = ROOT / "target" / ("native-installed-" + args.commit[:7] + ".json")
    output.write_text(json.dumps(result, indent=2) + "\n")
    print(output)


if __name__ == "__main__":
    main()
