#!/usr/bin/env python3
from pathlib import Path
import argparse
import json,subprocess,os,time,hashlib
parser=argparse.ArgumentParser(description='Exercise real native PCM output through the Doremi window. No microphone capture.')
parser.add_argument('binary',type=Path);parser.add_argument('output',type=Path);parser.add_argument('--commit',required=True)
args=parser.parse_args()
out=args.output.resolve();out.mkdir(parents=True,exist_ok=True)
binary=args.binary.resolve()
manifest=json.loads((binary.parent/'manifest.json').read_text())
assert manifest['product']=='Doremi' and manifest['source_commit']==args.commit
assert manifest['payload']['sha256']==hashlib.sha256(binary.read_bytes()).hexdigest()
env=os.environ.copy();env.pop('WAYLAND_DISPLAY',None);env.pop('WAYLAND_SOCKET',None);env['DOREMI_DATA_DIR']=str(out/'profile')
log=(out/'runtime.log').open('w');process=subprocess.Popen([str(binary)],env=env,stdout=log,stderr=subprocess.STDOUT)
def xdo(*args):return subprocess.check_output(['xdotool',*map(str,args)],text=True,timeout=15).strip()
def state():return json.loads((out/'profile/qa-state.json').read_text())
def wait(predicate):
 deadline=time.monotonic()+10
 while time.monotonic()<deadline:
  assert process.poll() is None,'Doremi exited'
  try:
   s=state()
   if predicate(s):return s
  except (FileNotFoundError,ValueError):pass
  time.sleep(.05)
 raise AssertionError('Expected native UI/audio state was not observed')
def tap(name):
 s=state();a,b,c,d=next(c['rect'] for c in s['controls'] if c['name']==name);scale=s['pixels_per_point'];assert 0<=a<c<=s['viewport'][0] and 0<=b<d<=s['viewport'][1],name;assert int(xdo('getwindowfocus'))==int(window),'Doremi lost focus';xdo('mousemove','--window',window,round((a+c)*scale/2),round((b+d)*scale/2),'click',1);time.sleep(.3)
def capture(name):
 subprocess.run(['import','-window',window,str(out/(name+'.png'))],check=True);(out/(name+'.json')).write_text(json.dumps(state(),indent=2))
try:
 window=xdo('search','--sync','--onlyvisible','--pid',process.pid).splitlines()[0];xdo('windowfocus','--sync',window);wait(lambda s:bool(s['controls']))
 tap('demo-examples');tap('local-audio');wait(lambda s:s['audio_metrics'] and s['audio_metrics']['frames']>10000 and s['playback']=='Playing');capture('playing')
 tap('pause');wait(lambda s:s['playback']=='Paused');time.sleep(.3);before=state()['position_ms'];time.sleep(.4);assert state()['position_ms']==before;capture('paused')
 current=state();rect=next(control['rect'] for control in current['controls'] if control['name']=='seek');scale=current['pixels_per_point']
 xdo('mousemove','--window',window,round((rect[0]+rect[2])*scale/2),round((rect[1]+rect[3])*scale/2),'mousedown',1);time.sleep(.1);xdo('mouseup',1)
 wait(lambda s:s['playback']=='Paused' and 3700<=s['position_ms']<=4300);capture('seek-paused')
 current=state();rect=next(control['rect'] for control in current['controls'] if control['name']=='volume')
 xdo('mousemove','--window',window,round((rect[0]+rect[2])*scale/2),round((rect[1]+rect[3])*scale/2),'click','--repeat',20,'--delay',15,5)
 wait(lambda s:s['volume']==0.0)
 xdo('mousemove','--window',window,round((rect[0]+(rect[2]-rect[0])*.7)*scale),round((rect[1]+rect[3])*scale/2),'mousedown',1);time.sleep(.1);xdo('mouseup',1)
 wait(lambda s:.65<s['volume']<.75);capture('volume')
 tap('play');wait(lambda s:s['playback']=='Playing');tap('next');wait(lambda s:s['current']=='tone-chord' and s['position_ms']>100);capture('next')
 tap('previous');wait(lambda s:s['current']=='tone-scale' and s['position_ms']>100);capture('previous')
 tap('stop');wait(lambda s:s['playback']=='Paused' and s['audio_metrics'] is None);capture('cancelled')
 (out/'report.json').write_text(json.dumps({'result':'pass','source_commit':args.commit,'platform':'Linux X11; native default audio device' ,'software_rendering':env.get('LIBGL_ALWAYS_SOFTWARE')=='1','input':'XTest mouse','acoustic_capture':False,'qa_script_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'checks':['UI Play delivers PCM','pause clock stable','seek paused through the real slider','volume wheel and pointer','resume','next','previous','cancel']},indent=2));print(out)
finally:
 process.terminate()
 try:process.wait(timeout=5)
 except subprocess.TimeoutExpired:process.kill();process.wait()
 log.close()
