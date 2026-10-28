#!/usr/bin/env python3
"""Original mathematical WAV fixtures for Doremi, dedicated to CC0-1.0."""
import hashlib
import json
import math
from pathlib import Path
import struct
import wave

RATE=48000
DURATION=8
ROOT=Path(__file__).resolve().parents[1]/'assets/audio'
ROOT.mkdir(parents=True,exist_ok=True)
manifest={'license':'CC0-1.0','origin':'Original mathematical synthesis by the Doremi development project. No recordings or third-party music.','sample_rate':RATE,'duration_ms':DURATION*1000,'files':[]}
for name,notes in [('scale',[261.6256,293.6648,329.6276,349.2282,391.9954,440.0,493.8833,523.2511]),('chord',[196.0,246.9417,293.6648,391.9954,293.6648,246.9417,196.0,261.6256])]:
    pcm=bytearray()
    for frame in range(RATE*DURATION):
        time=frame/RATE
        index=min(7,int(time))
        local=time-index
        envelope=min(1.0,local/.025,(1.0-local)/.08)
        fundamental=math.sin(2*math.pi*notes[index]*time)
        harmonic=.18*math.sin(2*math.pi*notes[index]*2*time)
        sample=round(32767*.18*max(0.0,envelope)*(fundamental+harmonic))
        pcm.extend(struct.pack('<h',sample))
    path=ROOT/(name+'.wav')
    with wave.open(str(path),'wb') as output:
        output.setnchannels(1);output.setsampwidth(2);output.setframerate(RATE);output.writeframes(pcm)
    manifest['files'].append({'file':path.name,'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'bytes':path.stat().st_size})
(ROOT/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps(manifest,indent=2))
