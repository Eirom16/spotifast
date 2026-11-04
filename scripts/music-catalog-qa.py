#!/usr/bin/env python3
"""Opt-in real MusicBrainz GUI journey under an isolated X11 display/profile.

Not run in ordinary CI: this makes public API requests (one at a time).
Screenshots and snapshots go only to the supplied evidence directory.
"""
import argparse
import ctypes
import ctypes.util
import json
import os
from pathlib import Path
import subprocess
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    profile = output / 'profile'
    env = os.environ.copy()
    env.pop('WAYLAND_DISPLAY', None)
    env.pop('WAYLAND_SOCKET', None)
    env['DOREMI_DATA_DIR'] = str(profile)
    log = (output / 'runtime.log').open('w')
    process = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=log, stderr=subprocess.STDOUT)

    def xdo(*args):
        return subprocess.check_output(['xdotool', *map(str, args)], text=True, timeout=15).strip()

    def state():
        return json.loads((profile / 'qa-state.json').read_text())

    def wait(predicate, seconds=22):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            assert process.poll() is None, 'Doremi exited'
            try:
                data = state()
                if predicate(data):
                    return data
            except (FileNotFoundError, ValueError):
                pass
            time.sleep(.1)
        raise AssertionError('GUI state did not arrive; inspect qa-state and runtime.log')

    window = None
    try:
        window = xdo('search', '--sync', '--onlyvisible', '--pid', process.pid).splitlines()[0]
        xdo('windowfocus', '--sync', window)
        wait(lambda data: bool(data['controls']))

        def tap(name):
            fixed={'home','search','library','queue','settings'}
            for _ in range(60):
                data=state()
                control=next(c for c in data['controls'] if c['name']==name)
                x1,y1,x2,y2=control['rect'];scale=data['pixels_per_point']
                left,top,right,bottom=data['page_rect']
                if name in fixed or top<=y1<y2<=bottom:
                    assert 0<=x1<x2<=data['viewport'][0],f'Control not visible: {name}'
                    xdo('mousemove','--window',window,round((x1+x2)*scale/2),round((y1+y2)*scale/2),'click',1)
                    time.sleep(.35)
                    return
                xdo('mousemove','--window',window,round((left+right)*scale/2),round((top+bottom)*scale/2),'click',4 if y1<top else 5)
                time.sleep(.15)
            raise AssertionError(f'Control did not scroll into view: {name}')

        def capture(name):
            subprocess.run(['import', '-window', window, str(output / f'{name}.png')], check=True, timeout=15)
            (output / f'{name}.json').write_text(json.dumps(state(), indent=2, ensure_ascii=False))

        def query(text):
            # xdotool's temporary Unicode mapping can race winit's keymap
            # update. Prepare it before input, exclusively on our Xvfb server.
            non_ascii=sorted({ord(character) for character in text if ord(character)>127})
            if non_ascii:
                servers=subprocess.check_output(['pgrep','-a','-x','Xvfb'],text=True).splitlines()
                display=env['DISPLAY'].split('.')[0]
                assert any(display in line.split() for line in servers),'Unicode QA requires an isolated Xvfb display'
                assert len(non_ascii)<=20
                x11=ctypes.CDLL(ctypes.util.find_library('X11'))
                x11.XOpenDisplay.argtypes=[ctypes.c_char_p];x11.XOpenDisplay.restype=ctypes.c_void_p
                x11.XChangeKeyboardMapping.argtypes=[ctypes.c_void_p,ctypes.c_int,ctypes.c_int,ctypes.POINTER(ctypes.c_ulong),ctypes.c_int]
                x11.XSync.argtypes=[ctypes.c_void_p,ctypes.c_int]
                x11.XCloseDisplay.argtypes=[ctypes.c_void_p]
                connection=x11.XOpenDisplay(None)
                assert connection
                try:
                    for offset,point in enumerate(non_ascii):
                        keysym=ctypes.c_ulong(point if point<=255 else 0x01000000|point)
                        x11.XChangeKeyboardMapping(connection,200+offset,1,ctypes.byref(keysym),1)
                    x11.XSync(connection,0)
                finally:
                    x11.XCloseDisplay(connection)
                time.sleep(.3)
            tap('query')
            xdo('key', '--clearmodifiers', 'ctrl+a')
            xdo('type', '--clearmodifiers', text)
            wait(lambda data: data['query'] == text)
            data = wait(lambda data: data['music_catalog']['selected'] and not data['music_catalog']['waiting'] and (data['music_catalog']['count'] is not None or data['music_catalog']['error'] is not None))
            assert data['music_catalog']['error'] is None, data['music_catalog']['error']
            return data

        tap('search')
        wait(lambda data: data['page'] == 'Search')
        assert state()['music_catalog']['selected']
        data = query('Moonlight Sonata')
        assert data['music_catalog']['count'] > 0
        capture('recordings-dark')
        first = next(c['name'] for c in data['controls'] if c['name'].startswith('catalog-open:recording:'))
        recording = first.removeprefix('catalog-open:')
        tap(first)
        wait(lambda data: data['music_catalog']['detail'] == recording)
        capture('recording-detail')
        tap('catalog-save:' + recording)
        wait(lambda data: any(item['provider'] == 'musicbrainz' and item['id'] == recording for item in data['saved_media']))
        tap('catalog-back')
        tap('catalog-kind:Artist')
        data = query('Björk')
        assert data['music_catalog']['count'] > 0
        capture('artists-unicode')
        artist = next(c['name'] for c in data['controls'] if c['name'].startswith('catalog-open:artist:'))
        tap(artist)
        wait(lambda data: not data['music_catalog']['waiting'] and data['music_catalog']['count'] is not None)
        assert state()['music_catalog']['error'] is None
        capture('artist-recordings')
        tap('catalog-back')
        tap('catalog-kind:Release')
        data = query('Kind of Blue')
        assert data['music_catalog']['count'] > 0
        release = next(c['name'] for c in data['controls'] if c['name'].startswith('catalog-open:release:'))
        tap(release)
        wait(lambda data: not data['music_catalog']['waiting'] and data['music_catalog']['count'] is not None)
        assert state()['music_catalog']['error'] is None
        capture('album-tracks')
        tap('catalog-back')
        data = query('doremiunlikelyqueryzz9876543210')
        assert data['music_catalog']['count'] == 0
        capture('empty-results')
        tap('query')
        xdo('key', '--clearmodifiers', 'ctrl+a')
        xdo('type', '--clearmodifiers', 'cancel this request')
        tap('cancel-search')
        wait(lambda data: data['query'] == '' and not data['music_catalog']['waiting'])
        time.sleep(1.2)
        assert state()['music_catalog']['count'] is None
        tap('library')
        wait(lambda data: data['page'] == 'Library')
        capture('saved-recording')
        (output / 'report.json').write_text(json.dumps({'result':'pass', 'platform':'Linux X11', 'recording':recording, 'input':'real XTest mouse/keyboard', 'audio':False, 'provider':'MusicBrainz metadata', 'checks':['recordings', 'artist recordings', 'release tracks', 'Unicode', 'empty results', 'cancel rejects late results', 'save stable ID']}, indent=2))
    except Exception:
        if window:
            subprocess.run(['import', '-window', window, str(output/'failure.png')], timeout=15, check=False)
        raise
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        log.close()


if __name__ == '__main__':
    main()
