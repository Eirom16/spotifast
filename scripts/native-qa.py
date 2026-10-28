#!/usr/bin/env python3
"""Exercise only the independent Doremi window under an isolated X11 display."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary',type=Path)
    parser.add_argument('output',type=Path)
    args=parser.parse_args()
    output=args.output.resolve(); output.mkdir(parents=True,exist_ok=True)
    directory=output/'profile'
    env=os.environ.copy(); env.pop('WAYLAND_DISPLAY',None);env.pop('WAYLAND_SOCKET',None)
    env['DOREMI_DATA_DIR']=str(directory)
    logfile=(output/'runtime.log').open('w')
    process=subprocess.Popen([str(args.binary.resolve())],env=env,stdout=logfile,stderr=subprocess.STDOUT)
    def xdo(*command):
        return subprocess.check_output(['xdotool',*map(str,command)],text=True,timeout=15).strip()
    def state(): return json.loads((directory/'qa-state.json').read_text())
    def wait(predicate):
        deadline=time.monotonic()+8
        while time.monotonic()<deadline:
            assert process.poll() is None,'Doremi exited unexpectedly'
            try:
                data=state()
                if predicate(data): return data
            except (FileNotFoundError,ValueError): pass
            time.sleep(.1)
        raise AssertionError('Expected UI state not observed')
    try:
        window=xdo('search','--sync','--onlyvisible','--pid',process.pid).splitlines()[0]
        xdo('windowfocus','--sync',window)
        wait(lambda s:bool(s['controls']))
        def tap(name):
            data=state(); control=next(c for c in data['controls'] if c['name']==name)
            x1,y1,x2,y2=control['rect']; scale=data['pixels_per_point']
            xdo('mousemove','--window',window,round((x1+x2)*scale/2),round((y1+y2)*scale/2),'click',1)
            time.sleep(.4)
        def capture(name):
            time.sleep(.2)
            subprocess.run(['import','-window',window,str(output/f'{name}.png')],check=True)
            (output/f'{name}.json').write_text(json.dumps(state(),indent=2,ensure_ascii=False))
        capture('home-dark')
        tap('save:dawn');wait(lambda s:'dawn' in s['library'])
        for name,page in [('library','Library'),('queue','Queue'),('settings','Settings')]:
            tap(name);wait(lambda s:s['page']==page);capture(name)
        tap('green');wait(lambda s:s['green']);capture('green-dark')
        tap('light');wait(lambda s:s['light']);capture('green-light')
        tap('purple');wait(lambda s:not s['green']);capture('purple-light')
        tap('search');wait(lambda s:s['page']=='Search');tap('query')
        xdo('type','--clearmodifiers','Lluvia');wait(lambda s:s['query']=='Lluvia');capture('search')
        tap('cancel-search');wait(lambda s:s['query']=='')
        tap('home');wait(lambda s:s['page']=='Home')
        tap('add:dawn');tap('add:dawn');wait(lambda s:len(s['manual'])==2)
        tap('select:rain');wait(lambda s:s['current']=='rain');capture('player')
        tap('next');wait(lambda s:s['current']=='dawn' and len(s['manual'])==1)
        tap('next');wait(lambda s:s['current']=='dawn' and len(s['manual'])==0)
        tap('next');wait(lambda s:s['current']=='cafe')
        tap('pause');wait(lambda s:s['playback']=='Paused')
        tap('shuffle');wait(lambda s:s['shuffle'])
        tap('repeat');wait(lambda s:s['repeat']=='Context')
        tap('stop');wait(lambda s:s['playback']=='Paused')
        capture('queue-sequence')
        (output/'report.json').write_text(json.dumps({'result':'pass','platform':'Linux X11','input':'XTest mouse and keyboard','audio':False,'sequence':'rain -> manual dawn -> manual dawn -> cafe'},indent=2))
        print(output)
    finally:
        process.terminate()
        try: process.wait(timeout=5)
        except subprocess.TimeoutExpired: process.kill();process.wait()
        logfile.close()

if __name__=='__main__': main()
