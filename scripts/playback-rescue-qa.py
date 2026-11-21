#!/usr/bin/env python3
"""Opt-in physical Android acceptance for the two maintainer-authorized IDs.

Requires an exact-commit CI APK and a new private QA profile. Never clears app
storage, reads credentials, copies the normal session, or changes the media IDs.
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

def adb(*args):
    return subprocess.check_output(['adb', '-d', *args], timeout=30)

def normal_hash(name):
    result = subprocess.run(['adb', '-d', 'shell', 'run-as', PACKAGE, 'sha256sum', 'files/' + name], capture_output=True, timeout=30)
    if result.returncode:
        assert b'No such file' in result.stderr
        return None
    return result.stdout.decode().split()[0]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('artifact', type=Path)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--profile', required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert re.fullmatch('[a-z0-9][a-z0-9-]{0,47}', args.profile)
    output = args.output.resolve()
    assert not output.is_relative_to(ROOT)
    output.mkdir(parents=True, exist_ok=False)
    (output / 'qa-script.py').write_bytes(Path(__file__).read_bytes())
    verifier = runpy.run_path(str(ROOT / 'scripts/android-qa.py'))
    apk = verifier['verify_payload'](args.artifact, args.commit)
    adb('install', '-r', str(apk))
    adb('install', '-r', '-t', str(args.artifact / 'app-debug-androidTest.apk'))
    private = f'files/qa/{args.profile}/'
    exists = subprocess.run(['adb', '-d', 'shell', 'run-as', PACKAGE, 'test', '-e', private], capture_output=True, timeout=30)
    assert exists.returncode == 1, 'Use a new isolated profile'
    protected = ['library-v1.json', 'youtube-session-v1.bin']
    before = {name: normal_hash(name) for name in protected}
    with verifier['qa_session'](args.profile):
        try:
            with (output / 'instrumentation.log').open('wb') as log:
                result = subprocess.run(['adb', '-d', 'shell', 'am', 'instrument', '-w', '-e', 'mode', 'rescue-playback', '-e', 'profile', args.profile, PACKAGE + '.test/dev.eirom.doremi.DoremiInstrumentation'], stdout=log, stderr=subprocess.STDOUT, timeout=2400)
            for name in ['qa-rescue-complete.json', 'qa-rescue-result.json', 'qa-state.json', 'qa-failure.json']:
                data = subprocess.run(['adb', '-d', 'shell', 'run-as', PACKAGE, 'cat', private + name], capture_output=True, timeout=30)
                if data.returncode == 0:
                    json.loads(data.stdout)
                    (output / name).write_bytes(data.stdout)
            log = (output / 'instrumentation.log').read_text()
            assert result.returncode == 0 and 'PASS:' in log and 'FAIL:' not in log, log
            report = json.loads((output / 'qa-rescue-result.json').read_text())
            assert report['result'] == 'pass' and len(report['complete']) == 4
            for identity in ['qQzdAsjWGPg', '_d3SaQI_Ag4']:
                assert sum(row['id'] == identity and row['eof'] for row in report['complete']) == 2
        finally:
            after = {name: normal_hash(name) for name in protected}
            (output / 'protected-data.json').write_text(json.dumps({'before': before, 'after': after}, indent=2))
            assert before == after, 'Normal-profile library/session changed during isolated QA'
    (output / 'artifact.json').write_text(json.dumps({'commit': args.commit, 'apk_sha256': hashlib.sha256(apk.read_bytes()).hexdigest(), 'result': 'pass'}, indent=2))
    print('PASS: whole corpus twice, controls and normal-profile hashes; see', output)

if __name__ == '__main__':
    main()
