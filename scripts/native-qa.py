#!/usr/bin/env python3
"""Exercise only the independent Doremi window under an isolated X11 display."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary',type=Path)
    parser.add_argument('output',type=Path)
    parser.add_argument('--size',type=int,nargs=2,metavar=('WIDTH','HEIGHT'))
    parser.add_argument('--playlist-music',action='store_true',help='Opt-in Commons requests and native playback of the audited CC0 music')
    args=parser.parse_args()
    output=args.output.resolve(); output.mkdir(parents=True,exist_ok=True)
    (output/'qa-script.py').write_bytes(Path(__file__).read_bytes())
    with args.binary.open('rb') as handle: binary_hash=hashlib.file_digest(handle,'sha256').hexdigest()
    directory=output/'profile'
    env=os.environ.copy(); env.pop('WAYLAND_DISPLAY',None);env.pop('WAYLAND_SOCKET',None)
    env['DOREMI_DATA_DIR']=str(directory)
    logfile=(output/'runtime.log').open('w')
    process=subprocess.Popen([str(args.binary.resolve())],env=env,stdout=logfile,stderr=subprocess.STDOUT)
    def xdo(*command):
        return subprocess.check_output(['xdotool',*map(str,command)],text=True,timeout=15).strip()
    def state(): return json.loads((directory/'qa-state.json').read_text())
    def wait(predicate,seconds=8):
        deadline=time.monotonic()+seconds
        while time.monotonic()<deadline:
            assert process.poll() is None,'Doremi exited unexpectedly'
            try:
                data=state()
                if predicate(data): return data
            except (FileNotFoundError,ValueError): pass
            time.sleep(.1)
        raise AssertionError('Expected UI state not observed')
    window=None
    try:
        window=xdo('search','--sync','--onlyvisible','--pid',process.pid).splitlines()[0]
        xdo('windowfocus','--sync',window)
        if args.size:
            xdo('windowsize','--sync',window,*args.size)
            wait(lambda s:all(abs(actual-expected)<1 for actual,expected in zip(s['viewport'],args.size)))
        wait(lambda s:bool(s['controls']))
        def tap(name):
            fixed={'home','search','library','queue','settings','previous','pause','play','next','stop','shuffle','repeat','player','seek','volume','player-more'}
            for attempt in range(45):
                data=state()
                if name.startswith('playlist-add:') and not any(c['name']==name for c in data['controls']):
                    media_id=name.split(':',2)[2]
                    if not any(c['name']=='playlist-choose:'+media_id for c in data['controls']):tap('row-more:'+media_id)
                    tap('playlist-choose:'+media_id);data=state()
                if name.startswith(('up:','down:','remove:')) and not any(c['name']==name for c in data['controls']):
                    occurrence=data['manual_occurrences'][int(name.split(':',1)[1])]
                    tap(f'queue-more:{occurrence}');data=state()
                if name.startswith(('playlist-up:','playlist-down:','playlist-remove:')) and not any(c['name']==name for c in data['controls']):
                    tap('playlist-row-more:'+name.split(':',1)[1]);data=state()
                if data['viewport'][0]<760 and data['page']!='Player' and name in {'previous','stop','shuffle','repeat','seek','volume'}:
                    tap('player');data=state()
                if not any(c['name']==name for c in data['controls']) and name.startswith(('add:','forget:','save-again:')):
                    tap('row-more:'+name.split(':',1)[1]);data=state()
                control=next(c for c in data['controls'] if c['name']==name)
                x1,y1,x2,y2=control['rect']; scale=data['pixels_per_point']
                left,top,right,bottom=data.get('page_rect',[0,0,*data['viewport']])
                is_menu=name.startswith(('add:','forget:','save-again:','playlist-add:','playlist-up:','playlist-down:','playlist-remove:','playlist-choose:','up:','down:','remove:'))
                is_fixed=name in {'home','search','library','queue','settings'} or (data['viewport'][0]>=760 and name in fixed) or (data['page']!='Player' and name in {'play','pause','next','player'})
                if is_menu or is_fixed or top<=y1<y2<=bottom:
                    assert 0<=x1<x2<=data['viewport'][0] and 0<=y1<y2<=data['viewport'][1],(name,control['rect'])
                    assert int(xdo('getwindowfocus'))==int(window), 'Own window lost focus; do not inject input'
                    xdo('mousemove','--window',window,round((x1+x2)*scale/2),round((y1+y2)*scale/2),'click',1)
                    time.sleep(.4)
                    return
                xdo('mousemove','--window',window,round((left+right)*scale/2),round((top+bottom)*scale/2),'click',4 if y1<top else 5)
                time.sleep(.15)
            raise AssertionError(f'Control did not scroll into the page: {name}')
        def capture(name):
            time.sleep(.2)
            subprocess.run(['import','-window',window,str(output/f'{name}.png')],check=True)
            (output/f'{name}.json').write_text(json.dumps(state(),indent=2,ensure_ascii=False))
        capture('home-dark')
        if state()['viewport'][0]>=760:
            tap('player');wait(lambda s:s['page']=='Player' and s['current'] is None)
            capture('player-empty')
            tap('player-search');wait(lambda s:s['page']=='Search');tap('home')
        tap('demo-examples');tap('save:dawn');wait(lambda s:'dawn' in s['library'])
        for name,page in [('library','Library'),('queue','Queue'),('settings','Settings')]:
            tap(name);wait(lambda s:s['page']==page);capture(name)
        tap('green');wait(lambda s:s['green']);capture('green-dark')
        tap('light');wait(lambda s:s['light']);capture('green-light')
        tap('purple');wait(lambda s:not s['green']);capture('purple-light')
        tap('search');wait(lambda s:s['page']=='Search');tap('source-demo');tap('query')
        xdo('type','--clearmodifiers','Lluvia');wait(lambda s:s['query']=='Lluvia');capture('search')
        tap('cancel-search');wait(lambda s:s['query']=='')
        tap('home');wait(lambda s:s['page']=='Home')
        tap('add:dawn');tap('add:dawn');wait(lambda s:len(s['manual'])==2)
        tap('add:cafe');wait(lambda s:len(s['manual'])==3)
        tap('select:rain');wait(lambda s:s['current']=='rain')
        tap('queue');wait(lambda s:s['page']=='Queue');capture('queue-occurrences')
        current=state()['current_occurrence'];context=state()['context_occurrences'];manual=state()['manual_occurrences']
        tap(f'queue-more:{manual[0]}');capture('queue-menu')
        tap('down:0');wait(lambda s:s['manual_occurrences']==[manual[1],manual[0],manual[2]])
        tap('up:1');wait(lambda s:s['manual_occurrences']==manual)
        tap('remove:1');wait(lambda s:s['manual_occurrences']==[manual[0],manual[2]])
        assert state()['current_occurrence']==current and state()['context_occurrences']==context
        capture('queue-edited')
        tap('clear');wait(lambda s:not s['manual'])
        assert state()['current_occurrence']==current and state()['context_occurrences']==context
        tap('queue-current');wait(lambda s:s['page']=='Player');capture('player')
        tap('home');tap('add:dawn');tap('add:dawn');wait(lambda s:len(s['manual'])==2)
        tap('player')
        tap('library');capture('mini-player-light');tap('player')
        tap('next');wait(lambda s:s['current']=='dawn' and len(s['manual'])==1)
        tap('next');wait(lambda s:s['current']=='dawn' and len(s['manual'])==0)
        tap('next');wait(lambda s:s['current']=='cafe')
        tap('pause');wait(lambda s:s['playback']=='Paused')
        tap('shuffle');wait(lambda s:s['shuffle'])
        tap('repeat');wait(lambda s:s['repeat']=='Context')
        tap('stop');wait(lambda s:s['playback']=='Paused')
        capture('queue-sequence')
        tap('settings');tap('dark');tap('player');capture('full-player-dark')
        tap('home');capture('mini-player-dark')
        # Actual user playlists share the production rows, queue and durable writer.
        tap('library');tap('playlist-name');xdo('type','--clearmodifiers','Café nocturno')
        tap('playlist-create');wait(lambda s:s['page']=='Playlist' and len(s['playlists'])==1)
        playlist_id=state()['playlists'][0]['id'];capture('playlist-empty')
        if state()['viewport'][0]>=760:
            tap('home');tap(f'sidebar-playlist:{playlist_id}');wait(lambda s:s['page']=='Playlist')
        first_id,second_id=('26896924','21923157') if args.playlist_music else ('dawn','cafe')
        if args.playlist_music:
            tap('home');tap('player')
            data=state();rect=next(c['rect'] for c in data['controls'] if c['name']=='volume');scale=data['pixels_per_point']
            xdo('mousemove','--window',window,round((rect[0]+(rect[2]-rect[0])*.08)*scale),round((rect[1]+rect[3])*scale/2),'mousedown',1)
            time.sleep(.15);xdo('mouseup',1);wait(lambda s:.02<s['volume']<.15)
            tap('search');tap('source-commons');tap('query');xdo('key','--clearmodifiers','ctrl+a');xdo('type','--clearmodifiers','Chopin')
            wait(lambda s:any(c['name']=='select:'+first_id for c in s['controls']),25)
        else: tap('home')
        for count in (1,2):
            tap(f'playlist-add:{playlist_id}:{first_id}');wait(lambda s:len(s['playlists'][0]['entries'])==count)
        if args.playlist_music:
            tap('query');xdo('key','--clearmodifiers','ctrl+a');xdo('type','--clearmodifiers','440')
            wait(lambda s:any(c['name']=='select:'+second_id for c in s['controls']),25)
        tap(f'playlist-add:{playlist_id}:{second_id}');wait(lambda s:len(s['playlists'][0]['entries'])==3)
        tap('library');tap(f'playlist-open:{playlist_id}');capture('playlist-three-entries')
        tap('playlist-select:1');wait(lambda s:s['current']==first_id and s['playback']=='Playing',25)
        assert state()['current_occurrence']==state()['context_occurrences'][1], 'Selected duplicate resolved to the wrong occurrence'
        if args.playlist_music:
            wait(lambda s:s['audio_metrics'] and s['audio_metrics']['nonzero_samples']>1000,25)
            capture('playlist-native-music')
        tap('pause');wait(lambda s:s['playback']=='Paused')
        tap('library');tap(f'playlist-open:{playlist_id}')
        checkpoint={key:state()[key] for key in ('current_occurrence','context_occurrences','manual_occurrences','upcoming')}
        tap('playlist-up:2');wait(lambda s:[e['id'] for e in s['playlists'][0]['entries']]==[first_id,second_id,first_id])
        tap('playlist-remove:1');wait(lambda s:[e['id'] for e in s['playlists'][0]['entries']]==[first_id,first_id])
        tap('playlist-edit');tap('playlist-rename');xdo('key','--clearmodifiers','ctrl+a');xdo('type','--clearmodifiers','Lectura nocturna')
        tap('playlist-rename-save');wait(lambda s:s['playlists'][0]['name']=='Lectura nocturna')
        for key,value in checkpoint.items():assert state()[key]==value, 'Editing a playlist changed the active queue: '+key
        capture('playlist-edited-dark')
        tap('settings');tap('light');tap('library');tap(f'playlist-open:{playlist_id}');capture('playlist-edited-light')
        wait(lambda s:json.loads((directory/'playlists-v1.json').read_text())['lists'][0]['name']=='Lectura nocturna')
        saved=state()['playlists'];old_pid=process.pid
        process.terminate();process.wait(timeout=8)
        process=subprocess.Popen([str(args.binary.resolve())],env=env,stdout=logfile,stderr=subprocess.STDOUT)
        window=xdo('search','--sync','--onlyvisible','--pid',process.pid).splitlines()[0];xdo('windowfocus','--sync',window)
        if args.size:
            xdo('windowsize','--sync',window,*args.size)
            wait(lambda s:all(abs(actual-expected)<1 for actual,expected in zip(s['viewport'],args.size)))
        restored=wait(lambda s:s['pid']!=old_pid and s['page']=='Home' and bool(s['controls']))
        assert restored['playlists']==saved and restored['playback']=='Paused' and restored['audio_metrics'] is None
        assert not restored['online']['worker_initialized'], 'Restoring a playlist started provider IO'
        tap('library');tap(f'playlist-open:{playlist_id}');capture('playlist-restored')
        tap('playlist-play');wait(lambda s:s['playback']=='Playing' and s['current']==first_id,25)
        if args.playlist_music:wait(lambda s:s['audio_metrics'] and s['audio_metrics']['nonzero_samples']>1000,25)
        tap('pause');tap('library');tap(f'playlist-open:{playlist_id}')
        before_delete={key:state()[key] for key in ('current_occurrence','context_occurrences','saved_media')}
        tap('playlist-edit');tap('playlist-delete');tap('playlist-delete-cancel')
        assert state()['playlists']==saved, 'Cancelling deletion changed the playlist'
        tap('playlist-delete');tap('playlist-delete-confirm');wait(lambda s:not s['playlists'] and s['page']=='Library')
        for key,value in before_delete.items():assert state()[key]==value, 'Deleting a playlist changed queue/favorites: '+key
        capture('playlist-deleted')
        (output/'report.json').write_text(json.dumps({'result':'pass','binary_sha256':binary_hash,'qa_script_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'platform':'Linux X11','input':'XTest mouse and keyboard','audio':bool(args.playlist_music),'acoustic_capture':False,'queue':['move exact duplicate down/up','remove second duplicate','clear manual while preserving current/context','open current player'],'playlists':['create Unicode name','append duplicate entries','select exact duplicate','reorder/remove','rename','preserve current queue','process restoration without IO','play without search','cancel deletion','delete while preserving queue/favorites'],'sequence':'rain -> manual dawn -> manual dawn -> cafe'},indent=2))
        print(output)
    except Exception:
        if window and process.poll() is None:
            try: capture('failure')
            except Exception as error: (output/'capture-failure.txt').write_text(str(error))
        raise
    finally:
        process.terminate()
        try: process.wait(timeout=5)
        except subprocess.TimeoutExpired: process.kill();process.wait()
        logfile.close()

if __name__=='__main__': main()
