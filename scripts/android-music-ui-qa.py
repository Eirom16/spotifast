#!/usr/bin/env python3
"""Opt-in physical Android UI and progressive CC0 music journey.

Uses a new private QA profile, positive low application gain, native callback
metrics and own-surface screenshots. Never clear data or change device settings.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import runpy
import subprocess

ROOT = Path(__file__).resolve().parents[1]
PACKAGE = 'dev.eirom.doremi.dev'
CAPTURES = ('qa-mobile-empty', 'qa-mobile-ime', 'qa-mobile-results-dark',
            'qa-mobile-results-light', 'qa-mobile-playing', 'qa-mobile-seek',
            'qa-mobile-mini', 'qa-mobile-library', 'qa-mobile-recreated',
            'qa-mobile-landscape', 'qa-mobile-cancelled')


def adb(*args):
    return subprocess.check_output(['adb', '-d', *args], timeout=420)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def normal_library():
    read = subprocess.run(['adb', '-d', 'shell', 'run-as', PACKAGE, 'cat',
                           'files/library-v1.json'], capture_output=True, timeout=30)
    if read.returncode == 0:
        return digest(read.stdout)
    assert b'No such file or directory' in read.stderr, read.stderr.decode(errors='replace')
    return None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('artifact', type=Path)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--profile', required=True, help='New private profile name; never reuse a failed run')
    args = parser.parse_args()
    assert re.fullmatch('[a-z0-9][a-z0-9-]{0,47}', args.profile)
    output = args.output.resolve()
    assert not output.is_relative_to(ROOT), 'Retain review screenshots outside the repository'
    output.mkdir(parents=True, exist_ok=False)
    verifier = runpy.run_path(str(ROOT/'scripts/android-qa.py'))
    apk = verifier['verify_payload'](args.artifact, args.commit)
    manifest = json.loads((args.artifact/'manifest.json').read_text())
    assert adb('get-state').decode().strip() == 'device'
    before = normal_library()
    (output/'normal-library-before.json').write_text(json.dumps({'sha256':before})+'\n')
    private = f'files/qa/{args.profile}/'
    exists = subprocess.run(['adb', '-d', 'shell', 'run-as', PACKAGE, 'test', '-e', private],
                            capture_output=True, timeout=30)
    assert exists.returncode == 1, 'Use a new private QA profile'
    adb('install', '-r', str(apk))
    installed = adb('shell', 'pm', 'path', PACKAGE).decode().strip()
    assert installed.startswith('package:') and '\n' not in installed and installed.endswith('/base.apk')
    assert digest(adb('exec-out', 'cat', installed.removeprefix('package:'))) == digest(apk.read_bytes())
    adb('install', '-r', '-t', str(args.artifact/'app-debug-androidTest.apk'))
    try:
        with verifier['qa_session'](args.profile):
            log = adb('shell', 'am', 'instrument', '-w', '-e', 'mode', 'music-ui',
                      '-e', 'profile', args.profile,
                      f'{PACKAGE}.test/dev.eirom.doremi.DoremiInstrumentation')
            (output/'instrumentation.log').write_bytes(log)
            latest = json.loads(adb('shell', 'run-as', PACKAGE, 'cat', private+'qa-state.json'))
            (output/'latest-state.json').write_text(json.dumps(latest, indent=2)+'\n')
            own_log = adb('logcat', '-d', f"--pid={int(latest['pid'])}", '-t', '1000').decode(errors='replace')
            (output/'own-process.log').write_text(own_log)
            # ADB exec-out does not carry the remote exit status. Use shell for
            # optional JSON, so a missing later capture cannot hide the real failure.
            for name in (*CAPTURES, 'qa-failure'):
                snapshot = subprocess.run(['adb', '-d', 'shell', 'run-as', PACKAGE, 'cat',
                                           private+name+'.json'], capture_output=True, timeout=30)
                if snapshot.returncode:
                    continue
                assert json.loads(snapshot.stdout)['pid'] == latest['pid']
                (output/(name+'.json')).write_bytes(snapshot.stdout)
                png = adb('exec-out', 'run-as', PACKAGE, 'cat', private+name+'.png')
                assert png.startswith(b'\x89PNG\r\n\x1a\n')
                (output/(name+'.png')).write_bytes(png)
            for name in ('qa-music-first-pcm', 'qa-mobile-checks'):
                saved = subprocess.run(['adb', '-d', 'shell', 'run-as', PACKAGE, 'cat',
                                        private+name+'.json'], capture_output=True, timeout=30)
                if saved.returncode == 0:
                    json.loads(saved.stdout)
                    (output/(name+'.json')).write_bytes(saved.stdout)
            assert log.decode().startswith('PASS: ') and 'FAIL:' not in log.decode(), log.decode()
            assert not re.search(r'FATAL EXCEPTION|Fatal signal \d+|panicked at|JNI DETECTED ERROR|shutdown.*timed out', own_log, re.IGNORECASE), 'Inspect own-process.log'
            checks = json.loads((output/'qa-mobile-checks.json').read_text())
            assert checks['result'] == 'pass' and 0 < checks['first_pcm_received_bytes'] < checks['total_bytes']
            assert all((output/(name+'.png')).is_file() for name in CAPTURES)
    finally:
        after = normal_library()
        (output/'normal-library-preserved.json').write_text(json.dumps({'before':before,'after':after,'preserved':after==before})+'\n')
        assert after == before, 'Normal-profile library changed'
    device = {key: adb('shell', 'getprop', key).decode().strip() for key in
              ('ro.product.model', 'ro.build.version.sdk', 'ro.product.cpu.abilist')}
    summary = dict(result='pass', source_commit=args.commit,
                   native_source_commit=manifest['native_source_commit'], apk_sha256=digest(apk.read_bytes()),
                   physical_device=True, device=device, normal_library_preserved=True,
                   acoustic_capture=False, checks=checks)
    (output/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    print(output)


if __name__ == '__main__':
    main()
