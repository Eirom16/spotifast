#!/usr/bin/env python3
"""Opt-in native GUI/audio journey using the audited Commons CC0 music selection.

Public requests; never run automatically in PR CI. Uses an isolated profile and
X11 display, at low application volume. Native callback evidence, no microphone.
"""
import argparse
import hashlib
import json
import os
import shutil
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary', type=Path)
parser.add_argument('output', type=Path)
args = parser.parse_args()
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
env = os.environ.copy()
env.pop('WAYLAND_DISPLAY', None)
env.pop('WAYLAND_SOCKET', None)
env['DOREMI_DATA_DIR'] = str(output/'profile')
log = (output/'runtime.log').open('w')
with args.binary.open('rb') as handle:
    binary_hash = hashlib.file_digest(handle, 'sha256').hexdigest()
shutil.copy2(__file__, output/'qa-script.py')
process = subprocess.Popen([str(args.binary.resolve())], env=env, stdout=log, stderr=subprocess.STDOUT)

def xdo(*command):
    return subprocess.check_output(['xdotool', *map(str, command)], text=True, timeout=15).strip()

def state():
    return json.loads((output/'profile/qa-state.json').read_text())

def wait(predicate, seconds=20):
    deadline = time.monotonic()+seconds
    while time.monotonic() < deadline:
        assert process.poll() is None, 'Doremi exited'
        try:
            data = state()
            if predicate(data):
                return data
        except (FileNotFoundError, ValueError):
            pass
        time.sleep(.05)
    raise AssertionError('Expected GUI/audio state absent; inspect evidence and runtime.log')

def pointer(name, fraction=.5):
    data = state()
    rect = next(item['rect'] for item in data['controls'] if item['name'] == name)
    a,b,c,d = rect
    assert 0 <= a < c <= data['viewport'][0] and 0 <= b < d <= data['viewport'][1], (name,rect)
    assert int(xdo('getwindowfocus')) == int(window), 'Own window lost focus'
    scale = data['pixels_per_point']
    xdo('mousemove','--window',window,round((a+(c-a)*fraction)*scale),round((b+d)*scale/2))

def tap(name):
    pointer(name)
    xdo('click',1)
    time.sleep(.3)

def slide(name,fraction):
    pointer(name,fraction)
    xdo('mousedown',1)
    time.sleep(.12)
    xdo('mouseup',1)

def capture(name):
    subprocess.run(['import','-window',window,str(output/(name+'.png'))],check=True,timeout=15)
    (output/(name+'.json')).write_text(json.dumps(state(),indent=2,ensure_ascii=False))

window = None
try:
    window = xdo('search','--sync','--onlyvisible','--pid',process.pid).splitlines()[0]
    xdo('windowfocus','--sync',window)
    wait(lambda s: bool(s['controls']))
    slide('volume',.08)
    wait(lambda s:.05 < s['volume'] < .11)
    tap('search')
    tap('source-commons')
    tap('query')
    xdo('type','--clearmodifiers','Chopin')
    wait(lambda s:any(c['name']=='select:26896924' for c in s['controls']))
    capture('real-music-search')
    tap('save:26896924')
    wait(lambda s:any(item['provider']=='wikimedia-commons' and item['id']=='26896924' for item in s['saved_media']))
    tap('select:26896924')
    first = wait(lambda s:s['playback']=='Playing' and s['audio_metrics'] and s['audio_metrics']['nonzero_samples']>1000)
    assert first['audio_metrics']['streaming']
    assert first['audio_metrics']['pcm_bytes'] <= 65536
    network = first['online']['network']
    assert 0 < network['received_bytes'] < network['total_bytes'], 'Complete file arrived before first observed native PCM'
    (output/'first-native-pcm.json').write_text(json.dumps(first,indent=2))
    capture('progressive-playing')
    tap('pause')
    wait(lambda s:s['playback']=='Paused')
    time.sleep(.3)
    before = state()['position_ms']
    time.sleep(.4)
    assert state()['position_ms']==before
    slide('seek',.5)
    wait(lambda s:s['playback']=='Paused' and 118000 <= s['position_ms'] <= 121000)
    capture('http-range-seek-paused')
    tap('play')
    wait(lambda s:s['playback']=='Playing' and s['position_ms']>120000)
    # A second explicitly selected CC0 resource verifies the real shared queue.
    tap('search')
    tap('query')
    xdo('key','--clearmodifiers','ctrl+a')
    xdo('type','--clearmodifiers','440')
    wait(lambda s:any(c['name']=='add:21923157' for c in s['controls']))
    tap('add:21923157')
    tap('player')
    tap('next')
    wait(lambda s:s['current']=='21923157' and s['playback']=='Playing' and s['audio_metrics']['nonzero_samples']>0)
    capture('next-real-audio')
    tap('previous')
    wait(lambda s:s['current']=='26896924' and s['playback']=='Playing' and s['audio_metrics']['nonzero_samples']>0)
    capture('previous-music')
    tap('stop')
    wait(lambda s:s['playback']=='Paused' and s['audio_metrics'] is None and s['online']['network'] is None)
    time.sleep(2.2)
    retired = state()['online']['retired_network']
    assert retired is not None
    time.sleep(.4)
    assert state()['online']['retired_network'] == retired, 'Cancelled source continued network work'
    capture('cancelled')
    report={'result':'pass','platform':'Linux X11/native audio callback','binary_sha256':binary_hash,'qa_script_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'acoustic_capture':False,'provider':'Commons audited CC0 recording','first_observed_pcm_received_bytes':network['received_bytes'],'total_encoded_bytes':network['total_bytes'],'checks':['real music search','save provider identity','PCM before whole file','bounded PCM','paused clock','HTTP seek while paused','resume','next authorized resource','previous music','cancel clears native output and stops network work']}
    (output/'report.json').write_text(json.dumps(report,indent=2))
except Exception:
    if window:
        capture('failure')
    raise
finally:
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()
    log.close()
