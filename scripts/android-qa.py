#!/usr/bin/env python3
"""ADB tests for the exclusive Doremi development app. Never clear app data."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time
import zipfile

PACKAGE='dev.eirom.doremi.dev'
ACTIVITY='dev.eirom.doremi.MainActivity'

def adb(*args, raw=False):
    return subprocess.check_output(['adb','-d',*args],text=not raw)

def state():
    return json.loads(adb('exec-out','run-as',PACKAGE,'cat','files/qa-state.json'))

def wait_for(predicate, seconds=8):
    deadline=time.monotonic()+seconds
    while time.monotonic()<deadline:
        try:
            data=state()
            if predicate(data): return data
        except (ValueError,subprocess.CalledProcessError): pass
        time.sleep(.2)
    raise AssertionError('State did not reach expected condition')

def verify_payload(directory, commit):
    manifest=json.loads((directory/'manifest.json').read_text())
    assert manifest['source_commit']==commit
    assert manifest['package']==PACKAGE and manifest['activity']==ACTIVITY
    assert manifest['min_sdk']==26 and manifest['target_sdk']==36
    for line in (directory/'SHA256SUMS').read_text().splitlines():
        digest,name=line.split('  ',1)
        assert Path(name).name==name
        assert hashlib.sha256((directory/name).read_bytes()).hexdigest()==digest
    apk=directory/'app-debug.apk'
    with zipfile.ZipFile(apk) as archive:
        assert archive.read('assets/SOURCE_COMMIT').decode().strip()==commit
        assert 'lib/armeabi-v7a/libdoremi_app.so' in archive.namelist()
        assert 'lib/arm64-v8a/libdoremi_app.so' in archive.namelist()
    return apk

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory',type=Path)
    parser.add_argument('--commit',required=True)
    parser.add_argument('--install',action='store_true')
    parser.add_argument('--origin-y',type=float,default=48)
    args=parser.parse_args()
    apk=verify_payload(args.directory,args.commit)
    output=Path('target/android-qa')/args.commit[:12]
    output.mkdir(parents=True,exist_ok=True)
    results=[]
    # -d refuses to operate when there is more than one USB target.
    assert adb('get-state').strip()=='device'
    properties={key:adb('shell','getprop',key).strip() for key in ['ro.product.manufacturer','ro.product.model','ro.build.version.sdk','ro.product.cpu.abilist']}
    if args.install:
        existing=adb('shell','pm','path',PACKAGE).strip()
        if existing:
            # A stable debug signing identity means Android itself rejects a
            # different signer. Never uninstall or clear data to defeat that.
            results.append({'test':'existing-package','result':'update preserving data'})
        adb('install','-r','-t',str(apk))
    launch=adb('shell','am','start','-W','-n',f'{PACKAGE}/{ACTIVITY}')
    (output/'launch.txt').write_text(launch)
    pid=int(adb('shell','pidof',PACKAGE).strip())
    wait_for(lambda s:bool(s['controls']) and s.get('pid')==pid)
    time.sleep(1)
    def capture(name):
        (output/f'{name}.png').write_bytes(adb('exec-out','screencap','-p',raw=True))
        (output/f'{name}.json').write_text(json.dumps(state(),indent=2,ensure_ascii=False))
    def tap(name):
        data=state()
        control=next(c for c in data['controls'] if c['name']==name)
        x1,y1,x2,y2=control['rect']; scale=data['pixels_per_point']
        x=(x1+x2)*scale/2; y=(y1+y2)*scale/2+args.origin_y
        adb('shell','input','tap',str(round(x)),str(round(y)))
        time.sleep(.7)
    def page(name, expected):
        tap(name); wait_for(lambda s:s['page']==expected); capture(name)
        results.append({'test':name,'result':'pass','observed_page':expected})
    capture('startup')
    page('home','Home')
    tap('save:dawn'); wait_for(lambda s:'dawn' in s['library'])
    page('library','Library')
    page('search','Search'); tap('query')
    adb('shell','input','text','Lluvia')
    wait_for(lambda s:s['query']=='Lluvia'); capture('search-ime')
    adb('shell','input','keyevent','4'); time.sleep(.5)
    tap('cancel-search'); wait_for(lambda s:s['query']=='')
    page('home','Home')
    before=len(state()['manual']); tap('add:dawn'); tap('add:dawn')
    wait_for(lambda s:len(s['manual'])==before+2)
    page('queue','Queue')
    tap(f'remove:{before}'); wait_for(lambda s:len(s['manual'])==before+1)
    page('settings','Settings')
    tap('green'); wait_for(lambda s:s['green']); capture('green-dark')
    tap('light'); wait_for(lambda s:s['light']); capture('green-light')
    tap('purple'); wait_for(lambda s:not s['green']); capture('purple-light')
    # Force stop only this app, without clearing its data.
    adb('shell','am','force-stop',PACKAGE)
    adb('shell','am','start','-W','-n',f'{PACKAGE}/{ACTIVITY}')
    pid=int(adb('shell','pidof',PACKAGE).strip())
    wait_for(lambda s:s.get('pid')==pid and s['page']=='Home' and s['light'] and not s['green'])
    page('settings','Settings'); capture('restored-preferences')
    tap('dark'); wait_for(lambda s:not s['light'])
    page('home','Home'); tap('select:dawn')
    wait_for(lambda s:s['page']=='Player' and s['current']=='dawn'); capture('player')
    tap('pause'); wait_for(lambda s:s['playback']=='Paused')
    tap('next'); wait_for(lambda s:s['current']=='rain')
    tap('previous'); wait_for(lambda s:s['current']=='dawn')
    tap('shuffle'); wait_for(lambda s:s['shuffle'])
    tap('repeat'); wait_for(lambda s:s['repeat']=='Context')
    tap('stop'); wait_for(lambda s:s['playback']=='Paused')
    adb('shell','input','keyevent','3'); time.sleep(.7)
    adb('shell','am','start','-W','-n',f'{PACKAGE}/{ACTIVITY}')
    wait_for(lambda s:s['current']=='dawn'); capture('foreground')
    pid=adb('shell','pidof',PACKAGE).strip(); assert pid
    (output/'meminfo.txt').write_text(adb('shell','dumpsys','meminfo',PACKAGE))
    (output/'gfxinfo.txt').write_text(adb('shell','dumpsys','gfxinfo',PACKAGE))
    (output/'logcat.txt').write_text(adb('logcat','-d','--pid='+pid,'-t','500'))
    (output/'report.json').write_text(json.dumps({'source_commit':args.commit,'device':properties,'results':results,'status':'pass','audio':False},indent=2))
    print(output)

if __name__=='__main__': main()
