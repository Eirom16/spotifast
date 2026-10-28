#!/usr/bin/env python3
"""Identify the independent native host, separately from the preserved legacy."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tomllib

binary=Path(sys.argv[1])
manifest=Path(sys.argv[2])
package=tomllib.loads(Path('crates/doremi-app/Cargo.toml').read_text())['package']
version=package['version']+'-dev'
assert subprocess.check_output([str(binary.resolve()),'--version'],text=True).strip()==f'Doremi Native Dev {version}'
assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip()
host=next(line.split(': ',1)[1] for line in subprocess.check_output(['rustc','-vV'],text=True).splitlines() if line.startswith('host: '))
manifest.write_text(json.dumps({'schema':1,'product':'Doremi','kind':'native-development','release':False,'version':version,'source_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'source_dirty':False,'target':host,'payload':{'file':binary.name,'sha256':hashlib.sha256(binary.read_bytes()).hexdigest()}},indent=2)+'\n')
