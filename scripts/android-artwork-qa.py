#!/usr/bin/env python3
"""Opt-in physical QA of exact-release artwork, using a new private profile."""
import argparse
import json
from pathlib import Path
import re
import runpy
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('artifact', type=Path)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--profile', required=True)
    args = parser.parse_args()
    assert re.fullmatch('[a-z0-9][a-z0-9-]{0,47}', args.profile)
    output = args.output.resolve()
    assert not output.is_relative_to(ROOT)
    output.mkdir(parents=True, exist_ok=False)
    (output/'qa-script.py').write_bytes(Path(__file__).read_bytes())
    common = runpy.run_path(str(ROOT/'scripts/android-music-ui-qa.py'))
    verifier = runpy.run_path(str(ROOT/'scripts/android-qa.py'))
    adb, digest, library = (common[name] for name in ('adb', 'digest', 'normal_library'))
    package = common['PACKAGE']
    apk = verifier['verify_payload'](args.artifact, args.commit)
    before = library()
    private = f'files/qa/{args.profile}/'
    exists = subprocess.run(['adb', '-d', 'shell', 'run-as', package, 'test', '-e', private], capture_output=True, timeout=30)
    assert exists.returncode == 1, 'Use a new private QA profile'
    adb('install', '-r', str(apk))
    installed = adb('shell', 'pm', 'path', package).decode().strip()
    assert installed.startswith('package:') and '\n' not in installed and installed.endswith('/base.apk')
    assert digest(adb('exec-out', 'cat', installed.removeprefix('package:'))) == digest(apk.read_bytes())
    adb('install', '-r', '-t', str(args.artifact/'app-debug-androidTest.apk'))
    try:
        with verifier['qa_session'](args.profile):
            log = adb('shell', 'am', 'instrument', '-w', '-e', 'mode', 'artwork-ui', '-e', 'profile', args.profile,
                      f'{package}.test/dev.eirom.doremi.DoremiInstrumentation')
            (output/'instrumentation.log').write_bytes(log)
            state = json.loads(adb('shell', 'run-as', package, 'cat', private+'qa-state.json'))
            (output/'latest-state.json').write_text(json.dumps(state, indent=2)+'\n')
            own_log = adb('logcat', '-d', f"--pid={int(state['pid'])}", '-t', '1000').decode(errors='replace')
            (output/'own-process.log').write_text(own_log)
            names = ('qa-artwork-results', 'qa-artwork-dark', 'qa-artwork-light', 'qa-artwork-recreated')
            for name in (*names, 'qa-failure'):
                saved = subprocess.run(['adb', '-d', 'shell', 'run-as', package, 'cat', private+name+'.json'], capture_output=True, timeout=30)
                if saved.returncode:
                    continue
                assert json.loads(saved.stdout)['pid'] == state['pid']
                (output/(name+'.json')).write_bytes(saved.stdout)
                png = adb('exec-out', 'run-as', package, 'cat', private+name+'.png')
                assert png.startswith(b'\x89PNG\r\n\x1a\n')
                (output/(name+'.png')).write_bytes(png)
            assert log.decode().startswith('PASS: ') and 'FAIL:' not in log.decode(), log.decode()
            assert not re.search(r'FATAL EXCEPTION|Fatal signal \d+|panicked at|JNI DETECTED ERROR', own_log)
            checks = json.loads(adb('shell', 'run-as', package, 'cat', private+'qa-artwork-checks.json'))
            assert checks['result'] == 'pass' and checks['cached_reopen'] and checks['activity_recreation'] and not checks['album_audio']
            assert all((output/(name+'.png')).is_file() for name in names)
            manifest = json.loads((args.artifact/'manifest.json').read_text())
            summary = dict(result='pass', source_commit=args.commit, native_source_commit=manifest['native_source_commit'],
                           apk_sha256=digest(apk.read_bytes()), physical_device=True, checks=checks,
                           device={key: adb('shell', 'getprop', key).decode().strip() for key in
                                   ('ro.product.model', 'ro.build.version.sdk', 'ro.product.cpu.abilist')})
    finally:
        after = library()
        (output/'normal-library-preserved.json').write_text(json.dumps({'before': before, 'after': after, 'preserved': before == after})+'\n')
        assert before == after, 'Normal-profile library changed'
    (output/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    print(output)


if __name__ == '__main__':
    main()
