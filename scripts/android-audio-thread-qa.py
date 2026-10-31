"""Own-process diagnostic, never inspect another application's threads."""
import argparse
import hashlib
from pathlib import Path
import json,subprocess,time,shlex,runpy
PKG='dev.eirom.doremi.dev'
ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('artifact',type=Path)
parser.add_argument('--commit',required=True)
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--held-seek',action='store_true',help='Also hold the seek at EOF for three seconds; the next occurrence must keep playing')
args=parser.parse_args()
OUT=args.output.resolve()
assert OUT.is_relative_to(ROOT/'target') and not OUT.exists()
OUT.parent.mkdir(parents=True,exist_ok=True)
geometry=json.loads((ROOT/'target/android-qa'/args.commit[:12]/'instrumentation/qa-startup.json').read_text())
origin=geometry['qa_surface_origin']
assert len(origin)==2 and all(isinstance(n,int) and n>=0 for n in origin)

def adb(*args): return subprocess.check_output(['adb','-d',*args],timeout=30)
def state():
 s=json.loads(adb('exec-out','run-as',PKG,'cat','files/qa-state.json'))
 assert s['viewport']==geometry['viewport'] and s['pixels_per_point']==geometry['pixels_per_point'],'Capture fresh portrait geometry from the same build first'
 return s
def wait(predicate,seconds=12):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  s=state()
  if predicate(s):return s
  time.sleep(.15)
 raise AssertionError('Doremi state did not arrive')
def tap(name):
 s=wait(lambda s:any(c['name']==name for c in s['controls']))
 focus=adb('shell','dumpsys','window','windows').decode()
 assert any('mCurrentFocus=' in l and PKG+'/' in l for l in focus.splitlines())
 rect=next(c['rect'] for c in s['controls'] if c['name']==name);scale=s['pixels_per_point']
 assert 0<=rect[0]<rect[2]<=s['viewport'][0] and 0<=rect[1]<rect[3]<=s['viewport'][1],name
 adb('shell','input','tap',str(round((rect[0]+rect[2])/2*scale+origin[0])),str(round((rect[1]+rect[3])/2*scale+origin[1])))
 time.sleep(.35)
def threads(pid):
 command=f'for task_dir in /proc/{int(pid)}/task/*; do cat "$task_dir/stat" 2>/dev/null; done'
 data=adb('shell','run-as '+PKG+' sh -c '+shlex.quote(command)).decode()
 result={}
 for line in data.splitlines():
  end=line.rindex(')');fields=line[end+2:].split();tid=int(line.split()[0])
  result[tid]=(line[line.index('(')+1:end],int(fields[11])+int(fields[12]))
 assert result,'Own thread snapshot unavailable'
 return result
def sample(label):
 s=state();pid=s['pid'];before=threads(pid);started=time.monotonic();time.sleep(2)
 after=threads(pid);elapsed=time.monotonic()-started;t=state()
 assert t['pid']==pid and t['current_occurrence']==s['current_occurrence'] and t['playback']==s['playback']
 if s['playback']=='Playing':assert t['audio_metrics']['frames']>s['audio_metrics']['frames']
 else:assert t['position_ms']==s['position_ms']
 entries=[dict(name=name,cpu_ticks=ticks-before[tid][1]) for tid,(name,ticks) in after.items() if tid in before and ticks>before[tid][1]]
 return dict(label=label,elapsed_ms=elapsed*1000,threads=sorted(entries,key=lambda e:-e['cpu_ticks']),metrics=t['audio_metrics'],playback=t['playback'],position_ms=t['position_ms'],new_threads=len(after.keys()-before.keys()))
apk=runpy.run_path(str(ROOT/'scripts/android-qa.py'))['verify_payload'](args.artifact,args.commit)
installed=adb('shell','pm','path',PKG).decode().strip()
assert installed.startswith('package:') and '\n' not in installed and installed.endswith('/base.apk')
assert hashlib.sha256(adb('exec-out','cat',installed.removeprefix('package:'))).digest()==hashlib.sha256(apk.read_bytes()).digest()
adb('shell','am','start','-W','-n',PKG+'/dev.eirom.doremi.MainActivity')
pid=int(adb('shell','pidof',PKG))
wait(lambda s:s['pid']==pid and bool(s['controls']))
assert 0.02<state()['volume']<0.15,'Run the low-gain instrumented suite first'
report=dict(source_commit=args.commit,model=adb('shell','getprop','ro.product.model').decode().strip(),method='own /proc/PID/task/*/stat utime+stime differences; tick frequency not inferred',acoustic_capture=False,samples=[])
try:
 tap('home');tap('local-audio')
 wait(lambda s:s['current']=='tone-scale' and s['playback']=='Playing' and s['position_ms']>700)
 report['samples'].append(sample('prepared-original'))
 tap('pause');wait(lambda s:s['playback']=='Paused');time.sleep(.3)
 report['samples'].append(sample('prepared-paused'))
 tap('home');tap('long-audio')
 wait(lambda s:s['current']=='tone-scale-long-v1' and s['playback']=='Playing' and s['position_ms']>700)
 for _ in range(3):report['samples'].append(sample('streamed-original'))
 tap('pause');wait(lambda s:s['playback']=='Paused');time.sleep(.3)
 report['samples'].append(sample('streamed-paused'))
 if args.held_seek:
  tap('play');wait(lambda s:s['current']=='tone-scale-long-v1' and s['playback']=='Playing')
  current=state();rect=next(c['rect'] for c in current['controls'] if c['name']=='seek')
  radius=(rect[3]-rect[1])/2.5*.75;scale=current['pixels_per_point']
  x=round((rect[0]+100-radius)*scale+origin[0]);y=round((rect[1]+rect[3])/2*scale+origin[1])
  focus=adb('shell','dumpsys','window','windows').decode()
  assert any('mCurrentFocus=' in l and PKG+'/' in l for l in focus.splitlines())
  adb('shell','input','swipe',str(x),str(y),str(x),str(y),'3000')
  ended=wait(lambda s:s['current']=='tone-chord' and s['playback']=='Playing' and (s['audio_metrics'] or {}).get('frames',0)>5000,4)
  assert ended['position_ms']<6000,'The new occurrence inherited the old seek gesture'
  report['held_seek']=dict(result='pass',hold_ms=3000,current=ended['current'],occurrence=ended['current_occurrence'],position_ms=ended['position_ms'],metrics=ended['audio_metrics'])
 tap('stop');tap('home');report['result']='pass'
except BaseException as error:
 report['result']='failed';report['error']=str(error)
 try:
  final=state();report['final_state']={key:final[key] for key in ('current','playback','position_ms','audio_metrics')}
 except Exception:
  pass
 adb('shell','am','force-stop',PKG)
 raise
finally:
 OUT.write_text(json.dumps(report,indent=2)+'\n')
print(OUT)
