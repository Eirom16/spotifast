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
import shlex
import subprocess
import time

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
    parser.add_argument('--playlists', action='store_true', help='Include own-playlist creation, editing and fresh-process playback')
    parser.add_argument('--query', default='Chopin')
    parser.add_argument('--id', default='26896924', help='Explicit Commons page ID')
    parser.add_argument('--duration-ms', type=int, default=239518, help='Expected duration from independently inspected provider metadata')
    args = parser.parse_args()
    assert re.fullmatch('[1-9][0-9]{0,19}', args.id) and 10000 < args.duration_ms <= 600000
    assert 0 < len(args.query) <= 128
    assert re.fullmatch('[a-z0-9][a-z0-9-]{0,47}', args.profile)
    output = args.output.resolve()
    assert not output.is_relative_to(ROOT), 'Retain review screenshots outside the repository'
    output.mkdir(parents=True, exist_ok=False)
    (output/'qa-script.py').write_bytes(Path(__file__).read_bytes())
    verifier = runpy.run_path(str(ROOT/'scripts/android-qa.py'))
    apk = verifier['verify_payload'](args.artifact, args.commit)
    manifest = json.loads((args.artifact/'manifest.json').read_text())
    assert adb('get-state').decode().strip() == 'device'
    before = normal_library()
    (output/'normal-library-before.json').write_text(json.dumps({'sha256':before})+'\n')
    captures = CAPTURES + (('qa-playlist-empty','qa-playlist-playing','qa-playlist-edited-dark','qa-playlist-edited-light') if args.playlists else ())
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
            log = adb('shell', 'am', 'instrument', '-w', '-e', 'mode', 'music-playlists' if args.playlists else 'music-ui',
                      '-e', 'profile', args.profile, '-e', 'musicId', args.id,
                      '-e', 'musicQuery', shlex.quote(args.query), '-e', 'musicDurationMs', str(args.duration_ms),
                      f'{PACKAGE}.test/dev.eirom.doremi.DoremiInstrumentation')
            (output/'instrumentation.log').write_bytes(log)
            latest = json.loads(adb('shell', 'run-as', PACKAGE, 'cat', private+'qa-state.json'))
            (output/'latest-state.json').write_text(json.dumps(latest, indent=2)+'\n')
            own_log = adb('logcat', '-d', f"--pid={int(latest['pid'])}", '-t', '1000').decode(errors='replace')
            (output/'own-process.log').write_text(own_log)
            # ADB exec-out does not carry the remote exit status. Use shell for
            # optional JSON, so a missing later capture cannot hide the real failure.
            for name in (*captures, 'qa-failure'):
                snapshot = subprocess.run(['adb', '-d', 'shell', 'run-as', PACKAGE, 'cat',
                                           private+name+'.json'], capture_output=True, timeout=30)
                if snapshot.returncode:
                    continue
                assert json.loads(snapshot.stdout)['pid'] == latest['pid']
                (output/(name+'.json')).write_bytes(snapshot.stdout)
                png = adb('exec-out', 'run-as', PACKAGE, 'cat', private+name+'.png')
                assert png.startswith(b'\x89PNG\r\n\x1a\n')
                (output/(name+'.png')).write_bytes(png)
            for name in ('qa-music-first-pcm', 'qa-mobile-checks', 'qa-playlist-checks'):
                saved = subprocess.run(['adb', '-d', 'shell', 'run-as', PACKAGE, 'cat',
                                        private+name+'.json'], capture_output=True, timeout=30)
                if saved.returncode == 0:
                    json.loads(saved.stdout)
                    (output/(name+'.json')).write_bytes(saved.stdout)
            assert log.decode().startswith('PASS: ') and 'FAIL:' not in log.decode(), log.decode()
            assert not re.search(r'FATAL EXCEPTION|Fatal signal \d+|panicked at|JNI DETECTED ERROR|shutdown.*timed out', own_log, re.IGNORECASE), 'Inspect own-process.log'
            checks = json.loads((output/'qa-mobile-checks.json').read_text())
            assert checks['result'] == 'pass' and 0 < checks['first_pcm_received_bytes'] < checks['total_bytes']
            assert all((output/(name+'.png')).is_file() for name in captures)
            if args.playlists:
                playlist_checks=json.loads((output/'qa-playlist-checks.json').read_text())
                assert playlist_checks['result']=='pass' and playlist_checks['queue_preserved'] and playlist_checks['unicode_ime']
            geometry = json.loads((output/'qa-mobile-cancelled.json').read_text())
            old_pid = latest['pid']
            adb('shell', 'am', 'force-stop', PACKAGE)
            adb('shell', 'am', 'start', '-W', '-n', PACKAGE+'/dev.eirom.doremi.MainActivity',
                '--es', 'dev.eirom.doremi.qa.profile', args.profile)

            def private_state():
                return json.loads(adb('shell', 'run-as', PACKAGE, 'cat', private+'qa-state.json'))

            def wait_for(predicate):
                deadline = time.monotonic()+25
                while time.monotonic() < deadline:
                    state = private_state()
                    if state['pid'] != old_pid and predicate(state):
                        return state
                    time.sleep(.1)
                raise AssertionError('Fresh-process music restoration did not reach the expected state')

            restored = wait_for(lambda state: state['page']=='Home' and bool(state['controls']))
            assert restored['saved_media'] == latest['saved_media']
            if args.playlists: assert restored['playlists'] == latest['playlists']
            assert restored['playback']=='Paused' and restored['audio_metrics'] is None
            assert not restored['online']['worker_initialized'], 'Restoration started provider IO automatically'
            (output/'restored-without-io.json').write_text(json.dumps(restored, indent=2)+'\n')

            def tap_restored(name):
                state = private_state()
                assert state['viewport']==geometry['viewport'] and state['pixels_per_point']==geometry['pixels_per_point'], 'Own surface geometry changed; do not inject input'
                focus = adb('shell', 'dumpsys', 'window', 'windows').decode()
                assert any('mCurrentFocus=' in line and PACKAGE+'/' in line for line in focus.splitlines()), 'No touch outside Doremi'
                rect = next(control['rect'] for control in state['controls'] if control['name']==name)
                a,b,c,d = rect
                assert 0<=a<c<=state['viewport'][0] and 0<=b<d<=state['viewport'][1]
                if name.startswith('select:'):
                    page = state['page_rect']
                    assert page[0]<=a<c<=page[2] and page[1]<=b<d<=page[3]
                origin = geometry['qa_surface_origin']
                scale = state['pixels_per_point']
                adb('shell', 'input', 'tap', str(round((a+c)*scale/2+origin[0])), str(round((b+d)*scale/2+origin[1])))
                time.sleep(.5)

            # The launch animation releases input after publishing the first frame.
            time.sleep(1)
            tap_restored('library')
            wait_for(lambda state:state['page']=='Library')
            if args.playlists:
                playlist_id=restored['playlists'][0]['id']
                tap_restored(f'playlist-open:{playlist_id}')
                wait_for(lambda state:state['page']=='Playlist')
                tap_restored('playlist-play')
            else:
                tap_restored('select:'+args.id)
            playing = wait_for(lambda state:state['current']==args.id and state['playback']=='Playing' and (state['audio_metrics'] or {}).get('nonzero_samples',0)>1000)
            assert .02<playing['volume']<.15 and playing['audio_metrics']['streaming']
            assert playing['query']=='' and playing['online']['search_source']=='demo' and playing['online']['search_status']=='Idle', 'Restoration depended on a preceding search'
            (output/'restored-native-music.json').write_text(json.dumps(playing, indent=2)+'\n')
            tap_restored('stop')
            wait_for(lambda state:state['playback']=='Paused' and state['audio_metrics'] is None)
    finally:
        after = normal_library()
        (output/'normal-library-preserved.json').write_text(json.dumps({'before':before,'after':after,'preserved':after==before})+'\n')
        assert after == before, 'Normal-profile library changed'
    device = {key: adb('shell', 'getprop', key).decode().strip() for key in
              ('ro.product.model', 'ro.build.version.sdk', 'ro.product.cpu.abilist')}
    summary = dict(result='pass', source_commit=args.commit,
                   native_source_commit=manifest['native_source_commit'], apk_sha256=digest(apk.read_bytes()),
                   physical_device=True, device=device, normal_library_preserved=True, selected_id=args.id, query=args.query, fixed_block_audit=args.id=='26896924',
                   acoustic_capture=False, playlists=bool(args.playlists), fresh_process_restore=True, playback_without_search=True,
                   qa_script_sha256=digest(Path(__file__).read_bytes()), checks=checks)
    (output/'summary.json').write_text(json.dumps(summary, indent=2)+'\n')
    print(output)


if __name__ == '__main__':
    main()
