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
    parser.add_argument('--artwork',action='store_true',help='Opt-in public metadata and licensed The Slip artwork; no album audio')
    parser.add_argument('--history-music',action='store_true',help='Opt-in actual CC0 listening, history, native replay and process restoration')
    parser.add_argument('--artist-releases',action='store_true',help='Opt-in real artist editions, paging, album tracks and back navigation; no audio')
    parser.add_argument('--catalog-library',action='store_true',help='Opt-in saved artist/album restoration by ID; no audio')
    parser.add_argument('--history-layout',type=Path,help='Offline geometry QA from an actual listening-history fixture directory')
    parser.add_argument('--close-helper',type=Path,help='Optional owned-window WM_DELETE_WINDOW helper; required by history QA')
    args=parser.parse_args()
    output=args.output.resolve(); output.mkdir(parents=True,exist_ok=True)
    (output/'qa-script.py').write_bytes(Path(__file__).read_bytes())
    with args.binary.open('rb') as handle: binary_hash=hashlib.file_digest(handle,'sha256').hexdigest()
    directory=output/'profile'
    if args.history_layout:
        directory.mkdir(exist_ok=False)
        for name in ['history-v1.json','queue-v1.json']:
            data=(args.history_layout/name).read_bytes();json.loads(data)
            (directory/name).write_bytes(data)
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
            fixed={'home','search','library','queue','settings','previous','pause','play','next','stop','shuffle','repeat','player','player-now-playing','seek','volume','player-more'}
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
                is_fixed=(name=='query' and data['viewport'][0]<760) or name in {'home','search','library','queue','settings'} or (data['viewport'][0]>=760 and name in fixed) or (data['page']!='Player' and name in {'play','pause','next','player'})
                if is_menu or is_fixed or top<=y1<y2<=bottom:
                    # During a native resize, an earlier frame's published
                    # controls can briefly precede the new viewport geometry.
                    # Wait within the existing bound; never inject outside it.
                    if not (0<=x1<x2<=data['viewport'][0] and 0<=y1<y2<=data['viewport'][1]):
                        time.sleep(.1)
                        continue
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
        if args.catalog_library:
            assert args.close_helper and args.close_helper.is_file()
            artist='artist:b7ffd2af-418f-4be2-bdd1-22f8b48613da';album='release:12b57d46-a192-499e-a91f-7da66790a1c1'
            def loaded(s):return not s['music_catalog']['waiting'] and (s['music_catalog']['count'] or 0)>0 and s['music_catalog']['error'] is None
            def saved(id):return {'provider':'musicbrainz','id':id} in state()['saved_media']
            tap('search');tap('source-musicbrainz');tap('catalog-kind:Release');tap('query');xdo('type','--clearmodifiers','The Slip')
            wait(lambda s:any(c['name']=='catalog-open:'+album for c in s['controls']),25);tap('catalog-open:'+album);wait(loaded,25)
            tap('catalog-save:'+album);wait(lambda s:saved(album))
            tap('catalog-artist:'+artist);wait(lambda s:loaded(s) and s['music_catalog']['detail']==artist,25)
            tap('catalog-save:'+artist);wait(lambda s:saved(artist));capture('saved-artist')
            tap('library');capture('saved-catalog-dark')
            assert len(state()['saved_media'])==2 and not state()['context_occurrences'] and not state()['manual_occurrences']
            tap('settings');tap('light');tap('library');capture('saved-catalog-light')
            wait(lambda s:len(json.loads((directory/'library-v1.json').read_text())['items'])==2)
            old_pid=process.pid;subprocess.run([str(args.close_helper.resolve()),str(window),str(old_pid)],check=True);assert process.wait(timeout=8)==0
            process=subprocess.Popen([str(args.binary.resolve())],env=env,stdout=logfile,stderr=subprocess.STDOUT)
            window=xdo('search','--sync','--onlyvisible','--pid',process.pid).splitlines()[0];xdo('windowfocus','--sync',window)
            if args.size:xdo('windowsize','--sync',window,*args.size)
            restored=wait(lambda s:s['pid']!=old_pid and s['page']=='Home' and bool(s['controls']) and (not args.size or all(abs(a-b)<1 for a,b in zip(s['viewport'],args.size))))
            assert len(restored['saved_media'])==2 and restored['query']=='' and not restored['online']['worker_initialized'] and restored['music_catalog']['generation']==0
            tap('library');capture('saved-catalog-restored');tap('select:'+artist)
            wait(lambda s:loaded(s) and s['music_catalog']['detail']==artist,25);capture('saved-artist-reopened')
            tap('library');tap('select:'+album);wait(lambda s:loaded(s) and s['music_catalog']['detail']==album,25)
            assert any(c['name']=='catalog-artist:'+artist for c in state()['controls']),'Album lookup lost its actual artist credits'
            capture('saved-album-reopened');tap('catalog-save:'+album);wait(lambda s:not saved(album))
            tap('library');capture('saved-album-removed')
            assert saved(artist) and len(state()['saved_media'])==1 and not state()['context_occurrences'] and not state()['audio'] and state()['query']==''
            subprocess.run([str(args.close_helper.resolve()),str(window),str(process.pid)],check=True);assert process.wait(timeout=8)==0
            (output/'report.json').write_text(json.dumps({'result':'pass','binary_sha256':binary_hash,'artist_id':artist,'release_id':album,'fresh_process':True,'restoration_without_network':True,'reopen_without_search':True,'album_metadata_refreshed':True,'remove_preserves_artist_and_queue':True,'audio':False,'themes':['dark','light']},indent=2)+'\n');print(output);return
        if args.artist_releases:
            artist='artist:b7ffd2af-418f-4be2-bdd1-22f8b48613da'
            def music():return state()['music_catalog']
            def loaded(s):return not s['music_catalog']['waiting'] and (s['music_catalog']['count'] or 0)>0 and s['music_catalog']['error'] is None
            tap('search');tap('source-musicbrainz');tap('catalog-kind:Artist');tap('query')
            xdo('type','--clearmodifiers','Nine Inch Nails')
            wait(lambda s:any(c['name']=='catalog-open:'+artist for c in s['controls']),25);tap('catalog-open:'+artist)
            wait(loaded,25);capture('artist-recordings')
            tap('artist-releases');wait(lambda s:loaded(s) and s['music_catalog']['artist_releases'],25)
            capture('artist-editions-dark')
            assert music()['detail']==artist and music()['offset']==0
            first_count=music()['count'];tap('catalog-next');wait(lambda s:loaded(s) and s['music_catalog']['offset']==first_count,25)
            capture('artist-editions-next');tap('catalog-previous');wait(lambda s:loaded(s) and s['music_catalog']['offset']==0,25)
            edition=next(c['name'] for c in state()['controls'] if c['name'].startswith('catalog-open:release:'))
            tap(edition);wait(lambda s:loaded(s) and s['music_catalog']['detail']==edition.removeprefix('catalog-open:'),25)
            capture('artist-album-tracks');tap('catalog-back');wait(lambda s:loaded(s) and s['music_catalog']['detail']==artist and s['music_catalog']['artist_releases'])
            tap('settings');tap('light');tap('search');capture('artist-editions-light')
            tap('artist-recordings');wait(lambda s:loaded(s) and not s['music_catalog']['artist_releases'],25)
            assert not state()['audio'] and state()['audio_metrics'] is None
            (output/'report.json').write_text(json.dumps({'result':'pass','binary_sha256':binary_hash,'artist_id':artist,'release_id':edition.removeprefix('catalog-open:'),'first_page_count':first_count,'actual_metadata':True,'paging_and_back':True,'album_tracks':True,'audio':False,'themes':['dark','light']},indent=2)+'\n')
            print(output)
            return
        if args.artwork:
            release='release:12b57d46-a192-499e-a91f-7da66790a1c1'
            tap('search');tap('source-musicbrainz');tap('catalog-kind:Release');tap('query')
            xdo('type','--clearmodifiers','The Slip')
            wait(lambda s:any(c['name']=='catalog-open:'+release for c in s['controls']),25)
            capture('album-results')
            tap('catalog-open:'+release)
            wait(lambda s:s['music_catalog']['detail']==release)
            capture('album-loading')
            loaded=wait(lambda s:s['music_catalog']['artwork']['dimensions'] is not None,25)
            art=loaded['music_catalog']['artwork']
            assert art['id']==release and all(0<n<=250 for n in art['dimensions']) and not art['error']
            assert loaded['current'] is None and loaded['audio_metrics'] is None
            time.sleep(.8);capture('album-dark')
            tap('settings');tap('light');tap('search')
            wait(lambda s:s['music_catalog']['artwork']['cache_hit'])
            capture('album-light-cached')
            tap('catalog-back');wait(lambda s:s['music_catalog']['detail'] is None and s['music_catalog']['artwork']['id'] is None)
            tap('catalog-open:'+release)
            wait(lambda s:s['music_catalog']['artwork']['cache_hit'])
            capture('album-reopened-cached')
            tap('settings');tap('dark');tap('search');wait(lambda s:s['music_catalog']['artwork']['cache_hit'])
            capture('album-dark-cached')
            (output/'report.json').write_text(json.dumps({
                'result':'pass','binary_sha256':binary_hash,'platform':'Linux X11',
                'qa_script_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                'release':release,'dimensions':art['dimensions'],'cached_reopen':True,'album_audio':False,
                'artwork_credit':'The Slip — art direction Rob Sheridan and Trent Reznor',
                'artwork_license':'CC BY-NC-SA 3.0 US; thumbnail resized for private noncommercial UI QA',
                'license_url':'https://creativecommons.org/licenses/by-nc-sa/3.0/us/',
                'license_evidence':'https://wiki.creativecommons.org/wiki/Nine_Inch_Nails_The_Slip',
                'source':'https://coverartarchive.org/release/12b57d46-a192-499e-a91f-7da66790a1c1/front-250',
            },indent=2)+'\n')
            print(output)
            return
        if args.history_layout:
            assert state()['listening_history']['count']>0 and not state()['online']['worker_initialized']
            for setting in ['dark','light']:
                tap('settings');tap(setting);tap('home');capture('recent-'+setting)
                rect=next(c['rect'] for c in state()['controls'] if c['name']=='history-open')
                assert rect[3]-rect[1]>=(48 if state()['viewport'][0]<760 else 32)
                tap('history-open');capture('history-'+setting)
                assert not state()['online']['worker_initialized']
            (output/'report.json').write_text(json.dumps({'result':'pass','binary_sha256':binary_hash,'fixture':str(args.history_layout.resolve()),'network':False,'audio':False,'viewport':state()['viewport'],'themes':['dark','light']},indent=2)+'\n')
            if args.close_helper:
                subprocess.run([str(args.close_helper.resolve()),str(window),str(process.pid)],check=True);assert process.wait(timeout=8)==0
            print(output)
            return
        if args.history_music:
            assert args.close_helper and args.close_helper.is_file()
            music='26896924'
            def count(s):return s['listening_history']['count']
            def close_history_window():
                began=time.monotonic();owned=process.pid
                subprocess.run([str(args.close_helper.resolve()),str(window),str(owned)],check=True)
                status=process.wait(timeout=8)
                assert status==0
                with (output/'window-closes.jsonl').open('a') as log:log.write(json.dumps({'pid':owned,'exit_code':status,'wm_delete_window':True,'seconds':time.monotonic()-began})+'\n')
            def slide_history(name,fraction):
                data=state();rect=next(c['rect'] for c in data['controls'] if c['name']==name);scale=data['pixels_per_point']
                assert int(xdo('getwindowfocus'))==int(window)
                xdo('mousemove','--window',window,round((rect[0]+(rect[2]-rect[0])*fraction)*scale),round((rect[1]+rect[3])*scale/2),'mousedown',1)
                time.sleep(.15);xdo('mouseup',1)
            assert count(state())==0
            tap('demo-examples');tap('select:dawn');tap('pause')
            if state()['viewport'][0]<760 and state()['page']!='Player':tap('player')
            slide_history('volume',.08);wait(lambda s:.02<s['volume']<.15)
            tap('search');tap('source-commons');tap('query');xdo('type','--clearmodifiers','Chopin')
            wait(lambda s:any(c['name']=='select:'+music for c in s['controls']),25)
            tap('select:'+music)
            if state()['page']!='Player':tap('player')
            first=wait(lambda s:s['playback']=='Playing' and s['audio_metrics'] and s['audio_metrics']['nonzero_samples']>1000,25)
            assert count(first)==0 and not first['saved_media']
            tap('pause');before_seek=state()['audio_metrics'];slide_history('seek',.6)
            wait(lambda s:s['playback']=='Paused' and s['position_ms']>100000 and s['audio_metrics'] is not None,25)
            assert count(state())==0,'Seek fabricated a listening record'
            tap('play')
            listened=wait(lambda s:count(s)==1,45)
            assert listened['audio_metrics']['frames']>=29*listened['audio_metrics']['output_rate']
            actual_seconds=before_seek['frames']/before_seek['output_rate']+listened['audio_metrics']['frames']/listened['audio_metrics']['output_rate']
            assert actual_seconds>=30,actual_seconds
            (output/'counted-native-frames.json').write_text(json.dumps(listened,indent=2)+'\n')
            tap('pause');tap('home');capture('recent-dark')
            assert state()['listening_history']['recent']==[{'provider':'wikimedia-commons','id':music}]
            tap('history-open');wait(lambda s:s['page']=='History');capture('history-dark')
            tap('history-clear');tap('history-clear-cancel');assert count(state())==1
            tap('settings');tap('light');tap('home');capture('recent-light');tap('history-open');capture('history-light')
            wait(lambda s:len(json.loads((directory/'history-v1.json').read_text())['plays'])==1)
            fixture=output/'restoration-fixture';fixture.mkdir()
            for name in ['history-v1.json','queue-v1.json']:(fixture/name).write_bytes((directory/name).read_bytes())
            close_history_window();old_pid=process.pid
            process=subprocess.Popen([str(args.binary.resolve())],env=env,stdout=logfile,stderr=subprocess.STDOUT)
            window=xdo('search','--sync','--onlyvisible','--pid',process.pid).splitlines()[0];xdo('windowfocus','--sync',window)
            if args.size:xdo('windowsize','--sync',window,*args.size)
            restored=wait(lambda s:s['pid']!=old_pid and s['page']=='Home' and bool(s['controls']) and (not args.size or all(abs(a-b)<1 for a,b in zip(s['viewport'],args.size))))
            assert count(restored)==1 and restored['query']=='' and not restored['online']['worker_initialized']
            capture('recent-restored');tap('history-play:'+music)
            played=wait(lambda s:s['current']==music and s['playback']=='Playing' and s['audio_metrics'] and s['audio_metrics']['nonzero_samples']>1000,25)
            assert played['query']=='' and count(played)==1
            tap('pause');tap('home');tap('history-open');tap('save:'+music)
            before={key:state()[key] for key in ['current_occurrence','context_occurrences','manual_occurrences','saved_media','playlists']}
            tap('history-clear');capture('history-clear-confirmation');tap('history-clear-confirm');wait(lambda s:count(s)==0)
            for key,value in before.items():assert state()[key]==value,key
            capture('history-cleared');wait(lambda s:not json.loads((directory/'history-v1.json').read_text())['plays'])
            close_history_window()
            (output/'report.json').write_text(json.dumps({'result':'pass','binary_sha256':binary_hash,'qa_script_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'source':'audited Commons CC0 Chopin by Membeth','id':music,'actual_native_listening_seconds':actual_seconds,'pre_seek_frames':before_seek['frames'],'post_seek_frames':listened['audio_metrics']['frames'],'seek_does_not_count':True,'separate_from_favorites':True,'fresh_process_without_io':True,'replay_without_search':True,'clear_preserves_queue_and_favorites':True,'acoustic_capture':False},indent=2)+'\n')
            print(output)
            return
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
        capture('search-idle')
        tap('query');xdo('type','--clearmodifiers','No matches in this demo')
        wait(lambda s:s['query']=='No matches in this demo' and not any(c['name'].startswith('select:') for c in s['controls']))
        capture('search-empty')
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
        if state()['viewport'][0]>=760:
            selected=state()['current'];was_saved=selected in state()['library']
            tap('player-save');wait(lambda s:(selected in s['library'])!=was_saved)
            tap('player-save');wait(lambda s:(selected in s['library'])==was_saved)
            tap('player-queue');wait(lambda s:s['page']=='Queue')
            tap('queue-current');wait(lambda s:s['page']=='Player')
            tap('player-back');wait(lambda s:s['page']=='Queue')
            tap('player-now-playing');wait(lambda s:s['page']=='Player')
            assert state()['current_occurrence']==current and state()['context_occurrences']==context
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
