#!/usr/bin/env python3
"""Validate APK identity, permissions, native alignment, assets and provenance."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import struct
import subprocess
import zipfile

PACKAGE = 'dev.eirom.doremi.dev'

def check(directory, create=False):
    sdk = Path(os.environ.get('ANDROID_HOME', '/opt/android-sdk'))
    tools = sdk / 'build-tools/36.0.0'
    apk = directory / 'app-debug.apk'
    badging = subprocess.check_output([str(tools / 'aapt'), 'dump', 'badging', str(apk)], text=True)
    assert f"package: name='{PACKAGE}'" in badging
    assert "sdkVersion:'26'" in badging and "targetSdkVersion:'36'" in badging
    assert "application-label:'Doremi'" in badging
    assert "launchable-activity: name='dev.eirom.doremi.MainActivity'" in badging
    assert 'uses-permission:' not in badging, badging
    assert "native-code: 'arm64-v8a' 'armeabi-v7a'" in badging
    subprocess.run([str(tools / 'apksigner'), 'verify', '--verbose', str(apk)], check=True)
    subprocess.run([str(tools / 'zipalign'), '-c', '-P', '16', '-v', '4', str(apk)], check=True, stdout=subprocess.DEVNULL)
    libraries = []
    with zipfile.ZipFile(apk) as archive:
        commit = archive.read('assets/SOURCE_COMMIT').decode().strip()
        assert re.fullmatch('[0-9a-f]{40}', commit)
        for name in ['assets/licenses/MIT.txt', 'assets/licenses/Doremi-assets-GPL-3.txt', 'assets/licenses/PROVENANCE.md']:
            assert len(archive.read(name)) > 0
        for name in archive.namelist():
            if not name.startswith('lib/') or not name.endswith('.so'):
                continue
            data = archive.read(name)
            assert data[:4] == b'\x7fELF'
            is64 = data[4] == 2
            machine = struct.unpack_from('<H', data, 18)[0]
            assert machine == (183 if 'arm64-v8a' in name else 40)
            offset = struct.unpack_from('<Q' if is64 else '<I', data, 32 if is64 else 28)[0]
            size, count = struct.unpack_from('<HH', data, 54 if is64 else 42)
            aligns = []
            for i in range(count):
                entry = offset + size * i
                if struct.unpack_from('<I', data, entry)[0] == 1:
                    alignment = struct.unpack_from('<Q' if is64 else '<I', data, entry + (48 if is64 else 28))[0]
                    aligns.append(alignment)
                    # Android's 16K requirement applies to 64-bit ABIs.
                    assert alignment >= (16384 if is64 else 4096), (name, alignment)
            libraries.append({'file': name, 'load_alignments': aligns})
    assert any('arm64-v8a/libdoremi_app.so' in item['file'] for item in libraries)
    assert any('armeabi-v7a/libdoremi_app.so' in item['file'] for item in libraries)
    sha = hashlib.sha256(apk.read_bytes()).hexdigest()
    manifest = {'schema': 1, 'source_commit': commit, 'package': PACKAGE, 'activity': 'dev.eirom.doremi.MainActivity', 'min_sdk':26, 'target_sdk':36, 'sha256':sha, 'libraries':libraries, 'release':False}
    path = directory / 'manifest.json'
    if create:
        assert commit == subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
        path.write_text(json.dumps(manifest, indent=2) + '\n')
        (directory / 'SHA256SUMS').write_text(''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n' for p in sorted(directory.glob('*.apk'))))
    else:
        assert manifest == json.loads(path.read_text())
    print(json.dumps(manifest,indent=2))

if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory',type=Path)
    parser.add_argument('--create',action='store_true')
    args=parser.parse_args()
    check(args.directory,args.create)
