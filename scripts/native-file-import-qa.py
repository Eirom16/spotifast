#!/usr/bin/env python3
"""Run real native file ingress with a private X11 display, bus and GTK data.

Only Doremi's original CC0 audio is generated/selected. Never repoint HOME,
connect to the user's session bus or collect another app's windows/history.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import wave

ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('binary',type=Path)
parser.add_argument('output',type=Path)
parser.add_argument('--commit',required=True)
args=parser.parse_args()
out=args.output.resolve()
assert out.is_relative_to(ROOT/'target') and not out.exists(),'Use a new target evidence directory'
out.mkdir(parents=True)
source=ROOT/'assets/audio/scale.wav'
manifest=json.loads((source.parent/'manifest.json').read_text())
expected=next(item for item in manifest['files'] if item['file']=='scale.wav')
assert manifest['license']=='CC0-1.0' and hashlib.sha256(source.read_bytes()).hexdigest()==expected['sha256']
with wave.open(str(source),'rb') as audio:
    assert audio.getnchannels()==1 and audio.getsampwidth()==2 and audio.getframerate()==48000
    original=audio.readframes(audio.getnframes())
# 16 kHz stereo exercises a different file layout from the mono built-in sample.
frames=b''.join(original[i:i+2]*2 for i in range(0,len(original),6))*15
fixture=out/'original-stereo-16k-120s.wav'
with wave.open(str(fixture),'wb') as audio:
    audio.setnchannels(2);audio.setsampwidth(2);audio.setframerate(16000);audio.writeframes(frames)
(out/'fixture-provenance.json').write_text(json.dumps(dict(source='assets/audio/scale.wav',source_sha256=expected['sha256'],license='CC0-1.0',derived_sha256=hashlib.sha256(fixture.read_bytes()).hexdigest(),channels=2,sample_rate=16000,duration_ms=120000),indent=2)+'\n')
bus=out/'session-bus.conf'
bus.write_text('''<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>
<policy context="default"><allow own="*"/><allow send_destination="*" eavesdrop="true"/><allow eavesdrop="true"/></policy>
</busconfig>
''')
env=os.environ.copy()
env.pop('WAYLAND_DISPLAY',None);env.pop('WAYLAND_SOCKET',None)
env['LIBGL_ALWAYS_SOFTWARE']='1'
subprocess.run(['xvfb-run','-a','-s','-screen 0 1440x1000x24','dbus-run-session','--config-file='+str(bus),'--',sys.executable,'-B',str(ROOT/'scripts/native-online-ui-qa.py'),str(args.binary.resolve()),str(out),'--commit',args.commit,'--local-import-only','--import-fixture',str(fixture)],env=env,check=True)
