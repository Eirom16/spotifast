#!/usr/bin/env python3
"""Permit reusing a CI native build only when every Rust input matches exactly."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

files=subprocess.check_output(['git','ls-files','-z','Cargo.toml','Cargo.lock','rust-toolchain.toml','.cargo','assets/doremi','assets/audio','scripts/build-android.sh','scripts/android-native-identity.py','crates/doremi-app','crates/doremi-core','crates/doremi-library','crates/doremi-player'],text=True).split('\0')
digest=hashlib.sha256()
for name in sorted(filter(None,files)):
    digest.update(name.encode()+b'\0'+Path(name).read_bytes()+b'\0')
directory=Path('target/android-jni')
manifest=directory/'native-source.json'
libs={str(p.relative_to(directory)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(directory.rglob('*.so'))}
assert libs
if '--check' in sys.argv:
    previous=json.loads(manifest.read_text())
    assert previous['rust_inputs_sha256']==digest.hexdigest()
    assert previous['libraries']==libs
else:
    manifest.write_text(json.dumps({'schema':1,'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'rust_inputs_sha256':digest.hexdigest(),'libraries':libs},indent=2)+'\n')
print('Native binary identity and Rust input fingerprint verified.')
