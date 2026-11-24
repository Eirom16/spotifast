#!/usr/bin/env python3
"""Real Samsung acceptance, private profile, exact-commit CI APK. No data clear."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import runpy
import subprocess
import time

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
    verifier = runpy.run_path(str(ROOT / 'scripts/android-qa.py'))
    apk = verifier['verify_payload'](args.artifact, args.commit)
    adb('install', '-r', str(apk))
    adb('install', '-r', '-t', str(args.artifact / 'app-debug-androidTest.apk'))
    private = f'files/qa/{args.profile}/'
    exists = subprocess.run(['adb', '-d', 'shell', 'run-as', PACKAGE, 'test', '-e', private], capture_output=True, timeout=30)
    assert exists.returncode == 1, 'Use a new isolated profile'
    protected = ['library-v1.json', 'youtube-session-v1.bin', 'playlists-v1.json', 'history-v1.json']
    before = {name: normal_hash(name) for name in protected}
    samples = []
    with verifier['qa_session'](args.profile):
        try:
            with (output / 'instrumentation.log').open('wb') as log:
                process = subprocess.Popen(['adb', '-d', 'shell', 'am', 'instrument', '-w', '-e', 'mode', 'background-playback', '-e', 'profile', args.profile, PACKAGE + '.test/dev.eirom.doremi.DoremiInstrumentation'], stdout=log, stderr=subprocess.STDOUT)
                deadline = time.monotonic() + 1800
                while process.poll() is None:
                    if time.monotonic() > deadline:
                        process.terminate()
                        raise TimeoutError('Bounded background QA expired')
                    time.sleep(5)
                    data = subprocess.run(['adb', '-d', 'exec-out', 'run-as', PACKAGE, 'cat', private + 'qa-state.json'], capture_output=True, timeout=30)
                    if data.returncode == 0:
                        state = json.loads(data.stdout)
                        sample = {k: state.get(k) for k in ('pid', 'instance', 'current', 'current_occurrence', 'playback', 'position_ms', 'audio_metrics', 'focus')}
                        power = adb('shell', 'dumpsys', 'power').decode()
                        sample['screen_off'] = 'Display Power: state=OFF' in power
                        sample['wakefulness'] = next((line.strip() for line in power.splitlines() if 'mWakefulness=' in line), '')
                        sample['wake_lease'] = [line.strip() for line in power.splitlines() if 'Doremi:playback' in line]
                        stats = subprocess.run(['adb', '-d', 'exec-out', 'run-as', PACKAGE, 'cat', '/proc/' + str(state['pid']) + '/stat'], capture_output=True, timeout=30)
                        if stats.returncode == 0:
                            fields = stats.stdout.decode().rsplit(')', 1)[1].split()
                            sample['cpu_ticks'] = int(fields[11]) + int(fields[12])
                        samples.append(sample)
                        if sample['screen_off']:
                            (output / 'screen-off-services.txt').write_bytes(adb('shell', 'dumpsys', 'activity', 'services', PACKAGE))
                assert process.returncode == 0
        finally:
            (output / 'samples.json').write_text(json.dumps(samples, indent=2))
            adb('shell', 'input', 'keyevent', '224')
            adb('shell', 'wm', 'dismiss-keyguard')
            listing = adb('shell', 'run-as', PACKAGE, 'ls', private).decode().splitlines()
            for name in listing:
                if name in {'qa-background-result.json', 'qa-state.json', 'qa-failure.json'} or re.fullmatch(r'qa-background-[a-z-]+\.png', name):
                    (output / name).write_bytes(adb('exec-out', 'run-as', PACKAGE, 'cat', private + name))
            (output / 'services-final.txt').write_bytes(adb('shell', 'dumpsys', 'activity', 'services', PACKAGE))
            (output / 'memory-final.txt').write_bytes(adb('shell', 'dumpsys', 'meminfo', PACKAGE))
            after = {name: normal_hash(name) for name in protected}
            (output / 'protected-data.json').write_text(json.dumps({'before': before, 'after': after}, indent=2))
            assert before == after, 'Normal-profile documents changed during isolated QA'
    log = (output / 'instrumentation.log').read_text()
    assert 'PASS:' in log and 'FAIL:' not in log, log
    report = json.loads((output / 'qa-background-result.json').read_text())
    assert report['result'] == 'pass' and report['background_eof']
    assert len([row for row in samples if row['screen_off'] and row['playback'] == 'Playing']) >= 40
    (output / 'artifact.json').write_text(json.dumps({'commit': args.commit, 'apk_sha256': hashlib.sha256(apk.read_bytes()).hexdigest(), 'result': 'pass', 'acoustic_capture': False}, indent=2))
    print('PASS: real device background acceptance; no acoustic capture; evidence:', output)


if __name__ == '__main__':
    main()
