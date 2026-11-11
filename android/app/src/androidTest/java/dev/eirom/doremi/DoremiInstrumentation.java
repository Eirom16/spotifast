package dev.eirom.doremi;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.content.pm.ActivityInfo;
import android.graphics.Bitmap;
import android.os.Bundle;
import android.os.SystemClock;
import android.view.MotionEvent;
import android.view.PixelCopy;
import android.view.SurfaceView;
import android.os.Handler;
import android.os.Looper;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import org.json.JSONArray;
import org.json.JSONObject;
import java.io.File;
import java.io.FileOutputStream;
import java.nio.file.Files;
import java.util.function.Predicate;

/** Physical IME, lifecycle and the same queue trace as the Linux host. */
public final class DoremiInstrumentation extends Instrumentation {
    private Activity activity;
    private String lastAction="startup";
    private JSONObject lastPointer=new JSONObject();
    private long settledInstance=-1;
    private int settledWidth=-1,settledHeight=-1;
    private String mode="full";
    private String profile="instrumentation";
    private String musicId="26896924",musicQuery="Chopin";
    private long musicDurationMs=239518;
    private String documentFolder="";
    private String documentTitle="Original Café estéreo",documentFilename="Original Café estéreo.wav";
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); profile=arguments==null?"instrumentation":arguments.getString("profile","instrumentation"); if(!profile.matches("[a-z0-9][a-z0-9-]{0,47}")) throw new IllegalArgumentException("Invalid QA profile"); mode=arguments==null?"full":arguments.getString("mode","full"); documentFolder=arguments==null?"":arguments.getString("documentFolder",""); if(arguments!=null && "flac".equals(arguments.getString("documentFormat","wav"))) {documentTitle="Original Café FLAC estéreo";documentFilename=documentTitle+".flac";} if(arguments!=null && "mp3".equals(arguments.getString("documentFormat","wav"))) {documentTitle="Original Café MP3 estéreo";documentFilename=documentTitle+".mp3";} configureMusic(arguments);start(); }
    private void configureMusic(Bundle arguments) {
        if(arguments==null) return;
        musicId=arguments.getString("musicId",musicId);musicQuery=arguments.getString("musicQuery",musicQuery);
        musicDurationMs=Long.parseLong(arguments.getString("musicDurationMs",Long.toString(musicDurationMs)));
        if(!musicId.matches("[1-9][0-9]{0,19}") || musicQuery.isEmpty() || musicQuery.length()>128 || musicDurationMs<=10000 || musicDurationMs>600000) throw new IllegalArgumentException("Invalid explicit music test selection");
    }
    private File profileDirectory() { return new File(getTargetContext().getFilesDir(),"qa/"+profile); }
    private JSONObject state() throws Exception {
        File path=new File(profileDirectory(),"qa-state.json");
        if(!path.isFile()) return new JSONObject();
        return new JSONObject(new String(Files.readAllBytes(path.toPath()),java.nio.charset.StandardCharsets.UTF_8));
    }
    private JSONObject until(Predicate<JSONObject> predicate) throws Exception {
        return until(8000,"fresh Rust state",predicate);
    }
    private JSONObject until(long timeout,String label,Predicate<JSONObject> predicate) throws Exception {
        long deadline=SystemClock.uptimeMillis()+timeout;
        while(SystemClock.uptimeMillis()<deadline) {
            JSONObject data=state();
            if(data.optInt("pid")==android.os.Process.myPid() && predicate.test(data)) return data;
            SystemClock.sleep(100);
        }
        JSONObject last=state();
        throw new AssertionError("Not observed: "+label+"; current="+last.optString("current")+" playback="+last.optString("playback")+" position_ms="+last.optLong("position_ms"));
    }
    private SurfaceView surface(View view) {
        if(view instanceof SurfaceView) return (SurfaceView)view;
        if(view instanceof ViewGroup) { ViewGroup group=(ViewGroup)view; for(int i=0;i<group.getChildCount();i++) { SurfaceView found=surface(group.getChildAt(i));if(found!=null) return found; } }
        return null;
    }
    private void foreground() throws Exception {
        long deadline=SystemClock.uptimeMillis()+6000;
        do {
            final boolean[] focused={false};final int[] size=new int[2];
            runOnMainSync(() -> {
                SurfaceView view=surface(activity.getWindow().getDecorView());
                focused[0]=!activity.isFinishing() && !activity.isDestroyed() && activity.hasWindowFocus() && activity.getWindow().getDecorView().isShown() && view!=null;
                if(view!=null) {size[0]=view.getWidth();size[1]=view.getHeight();}
            });
            if(focused[0]) {
                JSONObject current=state();float scale=(float)current.optDouble("pixels_per_point",1);
                JSONArray viewport=current.optJSONArray("viewport");
                boolean geometry=viewport!=null && size[0]>0 && size[1]>0 && Math.abs(viewport.optDouble(0)*scale-size[0])<2 && Math.abs(viewport.optDouble(1)*scale-size[1])<2;
                if(geometry) {
                    long instance=current.optLong("instance");
                    if(instance!=settledInstance || size[0]!=settledWidth || size[1]!=settledHeight) {
                        settledInstance=instance;settledWidth=size[0];settledHeight=size[1];
                        // Android publishes focus and GL geometry before its
                        // replacement/rotation animation releases touch input.
                        SystemClock.sleep(750);continue;
                    }
                    waitForIdleSync();return;
                }
            }
            SystemClock.sleep(80);
        } while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Doremi did not regain focus and matching GL geometry; no input injected");
    }
    private void settledForeground() throws Exception {
        foreground();
        // Focus can precede the end of Android window replacement animations.
        // Keep touch injection on our settled window after recreation/resume.
        SystemClock.sleep(750);foreground();
    }
    private void capture(String name) throws Exception {
        foreground();
        long deadline=SystemClock.uptimeMillis()+4000;int lastStatus=-1;
        do {
            final Bitmap[] image={null};final int[] status={-1};final int[] surfaceOrigin=new int[2],contentOrigin=new int[2];CountDownLatch copied=new CountDownLatch(1);
            runOnMainSync(() -> {
                SurfaceView view=surface(activity.getWindow().getDecorView());
                if(view==null || view.getWidth()==0 || view.getHeight()==0) {copied.countDown();return;}
                view.getLocationOnScreen(surfaceOrigin);activity.findViewById(android.R.id.content).getLocationOnScreen(contentOrigin);
                image[0]=Bitmap.createBitmap(view.getWidth(),view.getHeight(),Bitmap.Config.ARGB_8888);
                // State publication precedes EGL swap. After recreation the first
                // own surface buffer can legitimately be unavailable briefly.
                PixelCopy.request(view,image[0],value -> {status[0]=value;copied.countDown();},new Handler(Looper.getMainLooper()));
            });
            if(!copied.await(3,TimeUnit.SECONDS)) throw new AssertionError("Doremi capture timed out: "+name);
            lastStatus=status[0];
            if(lastStatus==PixelCopy.SUCCESS) {
                try(FileOutputStream file=new FileOutputStream(new File(profileDirectory(),name+".png"))) {image[0].compress(Bitmap.CompressFormat.PNG,100,file);}
                image[0].recycle();
                JSONObject snapshot=state();
                snapshot.put("qa_surface_origin",new JSONArray().put(surfaceOrigin[0]).put(surfaceOrigin[1]));
                snapshot.put("qa_content_origin",new JSONArray().put(contentOrigin[0]).put(contentOrigin[1]));
                Files.write(new File(profileDirectory(),name+".json").toPath(),snapshot.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
                return;
            }
            if(image[0]!=null) image[0].recycle();
            if(lastStatus!=PixelCopy.ERROR_SOURCE_NO_DATA && lastStatus!=PixelCopy.ERROR_TIMEOUT) break;
            SystemClock.sleep(120);
        } while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Doremi surface capture failed: "+name+" status="+lastStatus);
    }
    private void sendOwnPointerSync(MotionEvent event) throws Exception {
        event.setSource(android.view.InputDevice.SOURCE_TOUCHSCREEN);
        lastPointer=new JSONObject().put("pid",android.os.Process.myPid()).put("action",event.getActionMasked()).put("x",event.getX()).put("y",event.getY()).put("source",event.getSource());
        boolean delivered=automation().injectInputEvent(event,true);
        lastPointer.put("delivered",delivered);
        if(!delivered) throw new AssertionError("Doremi pointer injection was rejected");
    }
    private void openPlayer() throws Exception {
        if(!"Player".equals(state().optString("page"))) tap("player");
        until(s -> "Player".equals(s.optString("page")));
    }
    private boolean fixedControl(JSONObject data,String name) {
        if("query".equals(name) && data.optJSONArray("viewport").optDouble(0)<760) return true;
        if(java.util.Arrays.asList("home","search","library","queue","settings").contains(name)) return true;
        if(name.startsWith("up:") || name.startsWith("down:") || name.startsWith("remove:") || name.startsWith("add:") || name.startsWith("forget:") || name.startsWith("save-again:") || name.startsWith("playlist-add:") || name.startsWith("playlist-choose:") || name.startsWith("playlist-up:") || name.startsWith("playlist-down:") || name.startsWith("playlist-remove:")) return true;
        if(data.optJSONArray("viewport").optDouble(0)>=760) return java.util.Arrays.asList("play","pause","next","previous","stop","repeat","shuffle","seek","volume","player").contains(name);
        return !"Player".equals(data.optString("page")) && java.util.Arrays.asList("play","pause","next","player").contains(name);
    }
    private JSONArray visibleRect(String name) throws Exception {
        JSONObject data=state();JSONArray rect=controlRect(data,name);
        if(rect==null) throw new AssertionError("Missing control: "+name);
        for(int attempt=0;!fixedControl(data,name) && attempt<30;attempt++) {
            JSONArray page=data.optJSONArray("page_rect");
            if(page==null || (rect.getDouble(1)>=page.getDouble(1) && rect.getDouble(3)<=page.getDouble(3))) break;
            scrollPage(data,rect.getDouble(1)<page.getDouble(1));data=state();rect=controlRect(data,name);
            if(rect==null) throw new AssertionError("Control disappeared while scrolling: "+name);
        }
        JSONArray page=data.optJSONArray("page_rect");
        if(!fixedControl(data,name) && page!=null && (rect.getDouble(1)<page.getDouble(1) || rect.getDouble(3)>page.getDouble(3))) throw new AssertionError("Control did not scroll into its page: "+name);
        JSONArray viewport=data.getJSONArray("viewport");
        if(rect.getDouble(0)<0 || rect.getDouble(2)>viewport.getDouble(0) || rect.getDouble(1)<0 || rect.getDouble(3)>viewport.getDouble(1)) throw new AssertionError("Control outside Doremi viewport: "+name);
        return rect;
    }
    private JSONArray stableRect(String name) throws Exception {
        JSONArray previous=visibleRect(name);
        for(int sample=0;sample<10;sample++) {
            SystemClock.sleep(150);JSONArray current=visibleRect(name);
            boolean stable=true;
            for(int coordinate=0;coordinate<4;coordinate++) if(Math.abs(current.getDouble(coordinate)-previous.getDouble(coordinate))>0.25) stable=false;
            if(stable) return current;
            previous=current;
        }
        throw new AssertionError("Control did not settle before input: "+name);
    }
    private void tap(String name) throws Exception { tap(name,500); }
    private void tap(String name,long settle) throws Exception {
        lastAction="tap:"+name;
        foreground();
        JSONObject data=state();JSONArray rect=controlRect(data,name);
        if(rect==null && (name.startsWith("up:") || name.startsWith("down:") || name.startsWith("remove:"))) {
            int index=Integer.parseInt(name.substring(name.indexOf(':')+1));
            tap("queue-more:"+data.getJSONArray("manual_occurrences").getLong(index));data=state();rect=controlRect(data,name);
        }
        if(rect==null && name.startsWith("playlist-add:")) {
            String id=name.substring(name.indexOf(':',name.indexOf(':')+1)+1);
            if(!hasControl(data,"playlist-choose:"+id)) tap("row-more:"+id);
            tap("playlist-choose:"+id);data=state();rect=controlRect(data,name);
        }
        if(rect==null && (name.startsWith("playlist-up:") || name.startsWith("playlist-down:") || name.startsWith("playlist-remove:"))) {
            tap("playlist-row-more:"+name.substring(name.indexOf(':')+1));data=state();rect=controlRect(data,name);
        }
        if(rect==null && "Home".equals(data.optString("page")) && (name.startsWith("add:") || name.startsWith("select:") || name.startsWith("save:") || name.equals("local-audio") || name.equals("long-audio"))) {
            tap("demo-examples");data=state();rect=controlRect(data,name);
        }
        if(rect==null && (name.startsWith("add:") || name.startsWith("forget:") || name.startsWith("save-again:"))) {
            tap("row-more:"+name.substring(name.indexOf(':')+1));data=state();rect=controlRect(data,name);
        }
        if(rect==null && java.util.Arrays.asList("previous","stop","repeat","shuffle","seek","volume").contains(name) && data.optJSONArray("viewport").optDouble(0)<760) { openPlayer(); }
        rect=stableRect(name);data=state();
        lastAction="tap:"+name;
        final int[] origin=new int[2];
        runOnMainSync(() -> surface(activity.getWindow().getDecorView()).getLocationOnScreen(origin));
        float scale=(float)data.getDouble("pixels_per_point");
        float x=origin[0]+(float)(rect.getDouble(0)+rect.getDouble(2))*scale/2;
        float y=origin[1]+(float)(rect.getDouble(1)+rect.getDouble(3))*scale/2;
        long time=SystemClock.uptimeMillis();
        MotionEvent down=MotionEvent.obtain(time,time,MotionEvent.ACTION_DOWN,x,y,0); sendOwnPointerSync(down); down.recycle();
        // Hold across rendered frames; a future event timestamp alone does not
        // keep the native pointer down while egui evaluates slider interaction.
        SystemClock.sleep(80);
        MotionEvent up=MotionEvent.obtain(time,SystemClock.uptimeMillis(),MotionEvent.ACTION_UP,x,y,0); sendOwnPointerSync(up); up.recycle();
        SystemClock.sleep(settle);
    }
    private JSONArray controlRect(JSONObject data,String name) throws Exception {
        JSONArray controls=data.getJSONArray("controls");
        for(int i=0;i<controls.length();i++) {JSONObject control=controls.getJSONObject(i);if(name.equals(control.getString("name"))) return control.getJSONArray("rect");}
        return null;
    }
    private void scrollPage(JSONObject data,boolean upward) throws Exception {
        foreground();JSONArray page=data.getJSONArray("page_rect");final int[] origin=new int[2];
        runOnMainSync(() -> surface(activity.getWindow().getDecorView()).getLocationOnScreen(origin));
        float scale=(float)data.getDouble("pixels_per_point"),top=(float)page.getDouble(1),height=(float)(page.getDouble(3)-top);
        // Start in the page gutter; a drag beginning on a seek slider is its own gesture.
        float x=origin[0]+((float)page.getDouble(0)+4f)*scale;
        float from=origin[1]+(top+height*(upward?.25f:.8f))*scale,to=origin[1]+(top+height*(upward?.8f:.25f))*scale;
        long started=SystemClock.uptimeMillis();MotionEvent down=MotionEvent.obtain(started,started,MotionEvent.ACTION_DOWN,x,from,0);sendOwnPointerSync(down);down.recycle();
        for(int step=1;step<=6;step++) {SystemClock.sleep(30);MotionEvent move=MotionEvent.obtain(started,SystemClock.uptimeMillis(),MotionEvent.ACTION_MOVE,x,from+(to-from)*step/6,0);sendOwnPointerSync(move);move.recycle();}
        MotionEvent up=MotionEvent.obtain(started,SystemClock.uptimeMillis(),MotionEvent.ACTION_UP,x,to,0);sendOwnPointerSync(up);up.recycle();SystemClock.sleep(300);
    }
    private void slide(String name,float fraction) throws Exception {
        lastAction="slide:"+name+":"+fraction;
        foreground();
        if(controlRect(state(),name)==null) openPlayer();
        JSONArray rect=visibleRect(name);JSONObject data=state();
        final int[] origin=new int[2];runOnMainSync(() -> surface(activity.getWindow().getDecorView()).getLocationOnScreen(origin));
        float scale=(float)data.getDouble("pixels_per_point");
        // The shared Spotifast slider maps the complete recorded track rect.
        float left=(float)rect.getDouble(0),right=(float)rect.getDouble(2);
        float x=origin[0]+(left+(right-left)*fraction)*scale;
        float y=origin[1]+(float)(rect.getDouble(1)+rect.getDouble(3))*scale/2;
        long time=SystemClock.uptimeMillis();
        MotionEvent down=MotionEvent.obtain(time,time,MotionEvent.ACTION_DOWN,x,y,0);sendOwnPointerSync(down);down.recycle();
        // Hold across rendered frames; a future event timestamp alone does not
        // keep the native pointer down while egui evaluates slider interaction.
        SystemClock.sleep(80);
        MotionEvent up=MotionEvent.obtain(time,SystemClock.uptimeMillis(),MotionEvent.ACTION_UP,x,y,0);sendOwnPointerSync(up);up.recycle();SystemClock.sleep(500);
    }
    private void backgroundInput() throws Exception {
        tap("search");tap("query");text("Lluvia",true);
        runOnMainSync(() -> ((android.view.inputmethod.InputMethodManager)activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(activity.getWindow().getDecorView().getWindowToken(),0));
        SystemClock.sleep(400);long before=state().getLong("instance");
        runOnMainSync(() -> activity.moveTaskToBack(true));SystemClock.sleep(600);
        runOnMainSync(() -> getTargetContext().startActivity(new Intent(getTargetContext(),MainActivity.class).putExtra("dev.eirom.doremi.qa.profile",profile).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)));
        until(s -> s.optLong("instance")!=before && s.optString("query").equals("Lluvia"));
        tap("cancel-search");until(s -> s.optString("query").isEmpty());tap("query");
        runOnMainSync(() -> {
            InputConnection input=connection(activity.getWindow().getDecorView());
            if(input==null || !input.commitText("X",1)) throw new AssertionError("InputConnection unavailable after resume");
        });
        // Deliberately do not clear the editor from the test. Cancel must have
        // synchronized Android's editor, otherwise this becomes LluviaX.
        until(s -> s.optString("query").equals("X"));capture("qa-input-background");
        runOnMainSync(() -> ((android.view.inputmethod.InputMethodManager)activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(activity.getWindow().getDecorView().getWindowToken(),0));
        SystemClock.sleep(400);tap("cancel-search");
    }
    private void measureResources(String name) throws Exception {
        long cpuBefore=android.os.Process.getElapsedCpuTime(),wallBefore=SystemClock.elapsedRealtime();
        SystemClock.sleep(1100);
        long cpuMs=android.os.Process.getElapsedCpuTime()-cpuBefore,wallMs=SystemClock.elapsedRealtime()-wallBefore;
        android.os.Debug.MemoryInfo memory=new android.os.Debug.MemoryInfo();android.os.Debug.getMemoryInfo(memory);
        JSONObject resources=new JSONObject();resources.put("elapsed_ms",wallMs);resources.put("process_cpu_ms",cpuMs);
        resources.put("percent_of_one_cpu",100.0*cpuMs/wallMs);resources.put("total_pss_kib",memory.getTotalPss());
        resources.put("method","Process.getElapsedCpuTime / SystemClock.elapsedRealtime; own process only");
        JSONObject sample=state();
        resources.put("audio_metrics",sample.optJSONObject("audio_metrics"));resources.put("acoustic_capture",false);
        resources.put("volume",sample.optDouble("volume"));resources.put("playback",sample.optString("playback"));
        resources.put("current_provider",sample.optString("current_provider"));
        resources.put("condition",sample.optJSONObject("audio_metrics").optBoolean("streaming")?"native output from bounded local-file decode buffer; no acoustic capture":"native output from fully prepared PCM; no acoustic capture");
        Files.write(new File(profileDirectory(),name+".json").toPath(),resources.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }
    private void audio() throws Exception {
        tap("home");tap("local-audio");
        until(s -> s.optBoolean("audio") && s.optString("playback").equals("Playing") && s.optLong("position_ms")>300 && s.optJSONObject("audio_metrics")!=null && s.optJSONObject("audio_metrics").optLong("frames")>0);
        slide("volume",0.08f);until(s -> s.optDouble("volume",0)>0.02 && s.optDouble("volume",1)<0.15);
        capture("qa-audio-playing");
        measureResources("qa-audio-resources");
        tap("pause");until(s -> s.optString("playback").equals("Paused"));SystemClock.sleep(300);
        long paused=state().getLong("position_ms");SystemClock.sleep(400);
        if(state().getLong("position_ms")!=paused) throw new AssertionError("PCM advanced while paused");
        slide("seek",0.35f);until(s -> s.optString("playback").equals("Paused") && s.optLong("position_ms")>1000 && s.optLong("position_ms")<6500);
        slide("volume",0f);until(s -> s.optDouble("volume",1)<0.05);
        tap("play");until(s -> s.optString("playback").equals("Playing") && s.optJSONObject("audio_metrics").optLong("frames")>5000);
        slide("volume",0.7f);until(s -> s.optDouble("volume",0)>0.3);
        tap("next");until(s -> s.optString("current").equals("tone-chord") && s.optLong("position_ms")>100);
        tap("previous");until(s -> s.optString("current").equals("tone-scale") && s.optLong("position_ms")>100);
        // Request an actual competing focus grant, then release only our test grant.
        android.media.AudioManager manager=(android.media.AudioManager)getTargetContext().getSystemService(android.content.Context.AUDIO_SERVICE);
        android.media.AudioFocusRequest interruption=new android.media.AudioFocusRequest.Builder(android.media.AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
            .setAudioAttributes(new android.media.AudioAttributes.Builder().setUsage(android.media.AudioAttributes.USAGE_MEDIA).setContentType(android.media.AudioAttributes.CONTENT_TYPE_MUSIC).build())
            .setOnAudioFocusChangeListener(change -> {}).build();
        try {
            if(manager.requestAudioFocus(interruption)!=android.media.AudioManager.AUDIOFOCUS_REQUEST_GRANTED) throw new AssertionError("Test interruption focus denied");
            until(s -> s.optString("playback").equals("Paused") && !s.optBoolean("focus"));
        } finally { manager.abandonAudioFocusRequest(interruption); }
        SystemClock.sleep(300);if(!state().optString("playback").equals("Paused")) throw new AssertionError("Unexpected automatic resume after focus loss");
        capture("qa-audio-interrupted");
        tap("play");until(s -> s.optString("playback").equals("Playing"));
        // App-owned recreation must pause and retain the same occurrence.
        long occurrence=state().getLong("current_occurrence"),instance=state().getLong("instance");
        ActivityMonitor monitor=addMonitor(MainActivity.class.getName(),null,false);
        runOnMainSync(() -> activity.recreate());
        Activity recreated=waitForMonitorWithTimeout(monitor,5000);removeMonitor(monitor);
        if(recreated==null) throw new AssertionError("Audio recreation did not return an Activity");activity=recreated;
        until(s -> s.optLong("instance")!=instance && s.optLong("current_occurrence")==occurrence && s.optString("playback").equals("Paused"));
        settledForeground();
        tap("home");until(s -> "Home".equals(s.optString("page")));tap("local-audio");until(s -> s.optString("playback").equals("Playing"));
        long beforeBackground=state().getLong("instance");
        runOnMainSync(() -> activity.moveTaskToBack(true));SystemClock.sleep(700);
        runOnMainSync(() -> getTargetContext().startActivity(new Intent(getTargetContext(),MainActivity.class).putExtra("dev.eirom.doremi.qa.profile",profile).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)));
        until(s -> s.optLong("instance")!=beforeBackground && s.optString("playback").equals("Paused"));
        settledForeground();
        tap("home");until(s -> "Home".equals(s.optString("page")));tap("local-audio");until(s -> s.optString("playback").equals("Playing"));
        // Clear modes/manual entries so natural EOF must advance to the second tone.
        while(!state().optString("repeat").equals("Off")) tap("repeat");
        if(state().optBoolean("shuffle")) tap("shuffle");
        // The media lasts eight seconds; allow device preparation, rendering
        // and state-publication time as well. Still require the actual next ID.
        until(12000,"natural EOF of eight-second sample",s -> s.optString("current").equals("tone-chord") && s.optLong("position_ms")>100);
        capture("qa-audio-after-end");
        tap("stop");until(s -> s.optString("playback").equals("Paused") && s.isNull("audio_metrics"));
    }
    private void longAudio() throws Exception {
        // The preceding short-audio test leaves its player paused. Set a low
        // positive application gain before starting the longer original file.
        slide("volume",0.08f);tap("home");tap("long-audio");
        until(15000,"first long WAV output",s -> onlinePlaying(s,"tone-scale-long-v1") && s.optJSONObject("audio_metrics").optBoolean("streaming"));
        until(33000,"continuous WAV delivery beyond the old prepared-PCM limit",s -> s.optString("current").equals("tone-scale-long-v1") && s.optLong("position_ms")>23000);
        JSONObject metrics=state().getJSONObject("audio_metrics");
        if(metrics.getLong("pcm_bytes")!=65536 || metrics.getLong("buffer_capacity_frames")!=8192 || metrics.getLong("buffer_underruns")!=0 || metrics.getLong("nonzero_samples")==0) throw new AssertionError("Long WAV output was not bounded/nonzero/continuously fed");
        capture("qa-long-playing");measureResources("qa-long-resources");
        tap("pause");until(s -> "Paused".equals(s.optString("playback")));SystemClock.sleep(250);
        long position=state().getLong("position_ms");SystemClock.sleep(400);
        if(state().getLong("position_ms")!=position) throw new AssertionError("Paused file clock advanced");
        long occurrence=state().getLong("current_occurrence"),instance=state().getLong("instance");
        ActivityMonitor monitor=addMonitor(MainActivity.class.getName(),null,false);
        runOnMainSync(() -> activity.recreate());
        Activity recreated=waitForMonitorWithTimeout(monitor,5000);removeMonitor(monitor);
        if(recreated==null) throw new AssertionError("Long WAV recreation did not return");activity=recreated;
        until(s -> s.optLong("instance")!=instance && s.optLong("current_occurrence")==occurrence && "Paused".equals(s.optString("playback")) && s.isNull("audio_metrics"));
        capture("qa-long-recreated");
        slide("seek",0.75f);until(s -> "Paused".equals(s.optString("playback")) && s.optLong("position_ms")>85000 && s.optLong("position_ms")<95000);
        tap("play");until(s -> onlinePlaying(s,"tone-scale-long-v1"));capture("qa-long-after-seek");
        tap("next");until(s -> onlinePlaying(s,"tone-chord"));tap("previous");until(s -> onlinePlaying(s,"tone-scale-long-v1"));
        slide("seek",0.99f);until(8000,"long file EOF advances the shared queue",s -> onlinePlaying(s,"tone-chord"));capture("qa-long-after-end");
        tap("stop");until(s -> "Paused".equals(s.optString("playback")) && s.isNull("audio_metrics"));
    }
    private void hideKeyboard() throws Exception {
        runOnMainSync(() -> ((android.view.inputmethod.InputMethodManager)activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(activity.getWindow().getDecorView().getWindowToken(),0));
        SystemClock.sleep(400);
    }
    private boolean hasControl(JSONObject state,String name) {
        JSONArray controls=state.optJSONArray("controls");
        if(controls!=null) for(int i=0;i<controls.length();i++) if(name.equals(controls.optJSONObject(i).optString("name"))) return true;
        return false;
    }
    private boolean onlinePlaying(JSONObject s,String id) {
        JSONObject metrics=s.optJSONObject("audio_metrics");
        return id.equals(s.optString("current")) && "Playing".equals(s.optString("playback")) && metrics!=null && metrics.optLong("frames")>5000;
    }
    private void online() throws Exception {
        tap("search");tap("source-commons");tap("query");text("sine",true);hideKeyboard();
        until(12000,"real Commons results",s -> hasControl(s,"select:21923157") && hasControl(s,"select:21923199") && "Ready".equals(s.optJSONObject("online").optString("search_status")));
        capture("qa-online-results-dark");
        tap("settings");tap("light");tap("search");capture("qa-online-results-light");
        tap("settings");tap("dark");tap("search");
        // No prepared cache exists yet: observe a real pending load, then cancel.
        tap("select:21923157",0);
        until(2000,"online load before cancellation",s -> "Loading".equals(s.optString("playback")) && s.optJSONObject("online").optBoolean("loading"));
        tap("stop",0);until(s -> "Paused".equals(s.optString("playback")) && !s.optJSONObject("online").optBoolean("loading"));
        SystemClock.sleep(1200);
        if(!"Paused".equals(state().optString("playback")) || !state().isNull("audio_metrics")) throw new AssertionError("Cancelled online source became active");
        capture("qa-online-cancelled");
        slide("volume",0.08f);until(s -> s.optDouble("volume",0)>0.02 && s.optDouble("volume",1)<0.15);
        tap("play");until(15000,"online tone delivered to native output",s -> onlinePlaying(s,"21923157"));
        capture("qa-online-playing");
        measureResources("qa-online-resources");
        tap("pause");until(s -> "Paused".equals(s.optString("playback")));SystemClock.sleep(250);
        long paused=state().getLong("position_ms");SystemClock.sleep(350);
        if(state().getLong("position_ms")!=paused) throw new AssertionError("Online PCM advanced while paused");
        tap("play");until(4000,"online resume",s -> onlinePlaying(s,"21923157"));
        tap("next");until(15000,"second audited online tone",s -> onlinePlaying(s,"21923199"));
        tap("previous");until(4000,"previous online identity",s -> onlinePlaying(s,"21923157"));
        long occurrence=state().getLong("current_occurrence"),instance=state().getLong("instance");
        ActivityMonitor monitor=addMonitor(MainActivity.class.getName(),null,false);
        runOnMainSync(() -> activity.recreate());
        Activity recreated=waitForMonitorWithTimeout(monitor,5000);removeMonitor(monitor);
        if(recreated==null) throw new AssertionError("Online recreation did not return");activity=recreated;
        until(s -> s.optLong("instance")!=instance && s.optLong("current_occurrence")==occurrence && "Paused".equals(s.optString("playback")) && !s.optJSONObject("online").optBoolean("loading"));
        capture("qa-online-recreated");
        tap("play");until(4000,"online explicit resume after recreation",s -> onlinePlaying(s,"21923157"));
        long beforeBackground=state().getLong("instance");
        runOnMainSync(() -> activity.moveTaskToBack(true));SystemClock.sleep(700);
        runOnMainSync(() -> getTargetContext().startActivity(new Intent(getTargetContext(),MainActivity.class).putExtra("dev.eirom.doremi.qa.profile",profile).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)));
        until(s -> s.optLong("instance")!=beforeBackground && "Paused".equals(s.optString("playback")) && s.optLong("current_occurrence")==occurrence);
        capture("qa-online-background");
        tap("stop");tap("search");tap("source-demo");tap("cancel-search");
    }
    private int savedCount(JSONObject state,String id) {
        JSONArray saved=state.optJSONArray("saved_media");int count=0;
        if(saved!=null) for(int i=0;i<saved.length();i++) {
            JSONObject item=saved.optJSONObject(i);
            if("wikimedia-commons".equals(item.optString("provider")) && id.equals(item.optString("id"))) count++;
        }
        return count;
    }
    private JSONObject privateDocument(String name) throws Exception {
        return new JSONObject(new String(Files.readAllBytes(new File(profileDirectory(),name).toPath()),java.nio.charset.StandardCharsets.UTF_8));
    }
    private void savedLibrary() throws Exception {
        JSONObject before=state();String original=before.getJSONArray("saved_media").toString();
        boolean existed=savedCount(before,"21923157")>0;
        String legacy=privateDocument("preferences-v1.json").optJSONArray("library").toString();
        tap("search");tap("source-commons");tap("query");text("sine",true);hideKeyboard();
        until(12000,"Commons metadata for favorite",s -> "Ready".equals(s.optJSONObject("online").optString("search_status")));
        if(savedCount(state(),"21923157")==0) tap("save:21923157");
        tap("save-again:21923157");
        until(s -> savedCount(s,"21923157")==1);
        if(state().getJSONArray("saved_media").length()!=before.getJSONArray("saved_media").length()+(existed?0:1)) throw new AssertionError("Duplicate favorite or unrelated library edit");
        tap("library");capture("qa-library-saved");
        if(!existed) {
            // Remove only the test favorite we just added, never an existing one.
            tap("forget:21923157");until(s -> savedCount(s,"21923157")==0);
            if(!original.equals(state().getJSONArray("saved_media").toString())) throw new AssertionError("Removing test favorite changed existing favorites");
        }
        capture("qa-library-result");
        if(!existed) {tap("search");tap("save:21923157");until(s -> savedCount(s,"21923157")==1);}
        long deadline=SystemClock.uptimeMillis()+5000;boolean durable=false;
        do {
            JSONObject document=privateDocument("library-v1.json");
            JSONArray items=document.getJSONArray("items");int found=0;
            for(int i=0;i<items.length();i++) {
                JSONObject item=items.getJSONObject(i);
                if(item.length()!=4) throw new AssertionError("Unexpected library payload fields");
                if("wikimedia-commons".equals(item.optString("provider")) && "21923157".equals(item.optString("id"))) found++;
            }
            if(document.getInt("schema")==1 && found==1) {durable=true;break;}
            SystemClock.sleep(100);
        } while(SystemClock.uptimeMillis()<deadline);
        if(!durable) throw new AssertionError("Favorite did not reach versioned library file");
        if(!legacy.equals(privateDocument("preferences-v1.json").optJSONArray("library").toString())) throw new AssertionError("Legacy favorite archive was changed");
        JSONObject checks=new JSONObject();checks.put("duplicate_save_unique",true);checks.put("removed_owned_fixture",!existed);
        checks.put("preserved_existing_favorite",existed);checks.put("legacy_archive_preserved",true);checks.put("durable_schema",1);
        Files.write(new File(profileDirectory(),"qa-library-checks.json").toPath(),checks.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
        tap("home");
    }
    private void restoredLibrary() throws Exception {
        JSONObject initial=state();
        if(savedCount(initial,"21923157")!=1 || initial.optJSONObject("online").optBoolean("worker_initialized") || !initial.isNull("audio_metrics") || "Playing".equals(initial.optString("playback"))) throw new AssertionError("Library restore started IO/audio or lost qualified favorite");
        tap("home");openPlayer();slide("volume",0.08f);
        until(s -> s.optDouble("volume",0)>0.02 && s.optDouble("volume",1)<0.15);
        tap("library");capture("qa-library-restored");
        if(!"demo".equals(state().optJSONObject("online").optString("search_source")) || !"Idle".equals(state().optJSONObject("online").optString("search_status"))) throw new AssertionError("Saved playback must not require search results");
        tap("select:21923157");until(15000,"native output from restored favorite",s -> onlinePlaying(s,"21923157"));
        capture("qa-library-restored-audio");tap("pause");until(s -> "Paused".equals(s.optString("playback")));
    }
    private android.app.UiAutomation automation() {
        return getUiAutomation(android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES);
    }
    private boolean systemDocumentPicker(String owner) {
        return "com.android.documentsui".equals(owner) || "com.google.android.documentsui".equals(owner);
    }
    private android.view.accessibility.AccessibilityNodeInfo ownPickerNode(String text) throws Exception {
        long deadline=SystemClock.uptimeMillis()+8000;
        do {
            android.view.accessibility.AccessibilityNodeInfo root=automation().getRootInActiveWindow();
            if(root!=null && systemDocumentPicker(String.valueOf(root.getPackageName()))) {
                // Query only our exact folder/file names; never dump Recents,
                // screen contents or names belonging to the device's owner.
                boolean ownFolder=false;
                for(android.view.accessibility.AccessibilityNodeInfo node:root.findAccessibilityNodeInfosByText(documentFolder)) {
                    if(documentFolder.contentEquals(node.getText())) ownFolder=true;
                }
                if(ownFolder) for(android.view.accessibility.AccessibilityNodeInfo node:root.findAccessibilityNodeInfosByText(text)) {
                    if(text.contentEquals(node.getText())) return node;
                }
            }
            SystemClock.sleep(100);
        } while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Owned QA folder/file not visible; no other file selected or captured");
    }
    private void documentControl(String name) throws Exception {
        for(int attempt=0;attempt<8;attempt++) {
            JSONObject data=state();JSONArray controls=data.getJSONArray("controls");JSONArray rect=null;
            for(int i=0;i<controls.length();i++) if(name.equals(controls.getJSONObject(i).optString("name"))) rect=controls.getJSONObject(i).getJSONArray("rect");
            if(rect==null) throw new AssertionError("Missing document control");
            float height=(float)data.getJSONArray("viewport").getDouble(1);
            if(rect.getDouble(1)>=60 && rect.getDouble(3)<height-60) {tap(name);return;}
            foreground();
            final int[] origin=new int[2];runOnMainSync(() -> surface(activity.getWindow().getDecorView()).getLocationOnScreen(origin));
            float scale=(float)data.getDouble("pixels_per_point");
            float x=origin[0]+(float)data.getJSONArray("viewport").getDouble(0)*0.9f*scale;
            float from=height*(rect.getDouble(1)<60?0.35f:0.7f),to=height*(rect.getDouble(1)<60?0.7f:0.35f);
            long time=SystemClock.uptimeMillis();
            MotionEvent down=MotionEvent.obtain(time,time,MotionEvent.ACTION_DOWN,x,origin[1]+from*scale,0);sendOwnPointerSync(down);down.recycle();
            for(int step=1;step<=12;step++) {
                SystemClock.sleep(25);MotionEvent move=MotionEvent.obtain(time,SystemClock.uptimeMillis(),MotionEvent.ACTION_MOVE,x,origin[1]+(from+(to-from)*step/12)*scale,0);sendOwnPointerSync(move);move.recycle();
            }
            MotionEvent up=MotionEvent.obtain(time,SystemClock.uptimeMillis(),MotionEvent.ACTION_UP,x,origin[1]+to*scale,0);sendOwnPointerSync(up);up.recycle();SystemClock.sleep(250);
        }
        throw new AssertionError("Document control remained outside the visible content");
    }
    private void documentReturned(long previousInstance) throws Exception {
        foreground();
        until(data -> data.optLong("instance")!=previousInstance);
        // Window focus is delivered before the outgoing picker animation stops
        // intercepting touches on this Samsung/API 28. Do not grant injection
        // permission or send a touch into that foreign transition window.
        settledForeground();
    }
    private void cancelOwnDocumentPicker() throws Exception {
        long previousInstance=state().getLong("instance");
        // Own folder was verified immediately before entering this method.
        // Back navigates directories before it finishes DocumentsUI on API 28.
        for(int attempt=0;attempt<8;attempt++) {
            android.view.accessibility.AccessibilityNodeInfo root=automation().getRootInActiveWindow();
            String owner=root==null?"":String.valueOf(root.getPackageName());
            if(getTargetContext().getPackageName().equals(owner)) {documentReturned(previousInstance);return;}
            if(!systemDocumentPicker(owner)) throw new AssertionError("Owned document picker lost foreground; no key injected");
            long time=SystemClock.uptimeMillis();
            automation().injectInputEvent(new android.view.KeyEvent(time,time,android.view.KeyEvent.ACTION_DOWN,android.view.KeyEvent.KEYCODE_BACK,0),true);
            SystemClock.sleep(80);
            automation().injectInputEvent(new android.view.KeyEvent(time,SystemClock.uptimeMillis(),android.view.KeyEvent.ACTION_UP,android.view.KeyEvent.KEYCODE_BACK,0),true);
            SystemClock.sleep(250);
        }
        throw new AssertionError("Owned selector did not finish after bounded Back navigation");
    }
    private void chooseOwnDocument(String filename) throws Exception {
        lastAction="select-owned-document";
        long previousInstance=state().getLong("instance");
        documentControl("import-wav");
        android.view.accessibility.AccessibilityNodeInfo node=ownPickerNode(filename);
        android.graphics.Rect bounds=new android.graphics.Rect();node.getBoundsInScreen(bounds);
        if(bounds.isEmpty()) throw new AssertionError("Owned file has no touch bounds");
        long time=SystemClock.uptimeMillis();
        MotionEvent down=MotionEvent.obtain(time,time,MotionEvent.ACTION_DOWN,bounds.centerX(),bounds.centerY(),0);
        down.setSource(android.view.InputDevice.SOURCE_TOUCHSCREEN);
        if(!automation().injectInputEvent(down,true)) throw new AssertionError("Owned picker touch rejected");down.recycle();
        SystemClock.sleep(80);
        MotionEvent up=MotionEvent.obtain(time,SystemClock.uptimeMillis(),MotionEvent.ACTION_UP,bounds.centerX(),bounds.centerY(),0);
        up.setSource(android.view.InputDevice.SOURCE_TOUCHSCREEN);
        if(!automation().injectInputEvent(up,true)) throw new AssertionError("Owned picker release rejected");up.recycle();
        documentReturned(previousInstance);
        until(15000,"selected document completion",data -> !data.optJSONObject("local_import").optBoolean("pending"));
    }
    private String savedDocument(JSONObject data) throws Exception {
        // qa-state deliberately exposes only qualified IDs. Check the durable
        // collection for metadata, then require the same ID in the live model.
        JSONArray items=privateDocument("library-v1.json").getJSONArray("items");
        for(int i=0;i<items.length();i++) {
            JSONObject item=items.getJSONObject(i);
            if("local-files".equals(item.optString("provider")) && documentTitle.equals(item.optString("title"))) {
                JSONArray saved=data.getJSONArray("saved_media");
                for(int j=0;j<saved.length();j++) if(item.getString("id").equals(saved.getJSONObject(j).optString("id")) && "local-files".equals(saved.getJSONObject(j).optString("provider"))) return item.getString("id");
            }
        }
        return "";
    }
    private void documentImport(boolean restore) throws Exception {
        if(!restore && !documentFolder.matches("Doremi-QA-[0-9a-f]{7,40}")) throw new AssertionError("A private QA folder is required");
        JSONObject initial=state();String originalLibrary=initial.getJSONArray("saved_media").toString();
        if(restore && (savedDocument(initial).isEmpty() || !initial.isNull("audio_metrics") || initial.getJSONObject("online").optBoolean("worker_initialized") || "Playing".equals(initial.optString("playback")))) throw new AssertionError("Restoration lost the document or started automatic IO");
        tap("home");
        if(!restore) {
            // A new device, or a preceding failed suite, can retain a simulated
            // demo selection. Its player intentionally has no audio sliders.
            tap("local-audio");until(data -> "Playing".equals(data.optString("playback")));tap("pause");
        } else {openPlayer();}
        slide("volume",0.08f);tap("library");
        if(!restore) {
            documentControl("import-wav");ownPickerNode("Invalid.wav");
            cancelOwnDocumentPicker();until(data -> !data.optJSONObject("local_import").optBoolean("pending"));
            if(!originalLibrary.equals(state().getJSONArray("saved_media").toString())) throw new AssertionError("Selector cancellation changed existing favorites");
            chooseOwnDocument("Invalid.wav");until(data -> data.optString("notice").contains("no es WAV"));
            if(!originalLibrary.equals(state().getJSONArray("saved_media").toString())) throw new AssertionError("An invalid document changed existing favorites");
            boolean visibleError=false;JSONObject invalidState=state();JSONArray invalidControls=invalidState.getJSONArray("controls");
            for(int i=0;i<invalidControls.length();i++) {
                JSONObject control=invalidControls.getJSONObject(i);JSONArray rect=control.getJSONArray("rect");
                if("import-feedback".equals(control.optString("name")) && rect.getDouble(1)>=60 && rect.getDouble(3)<invalidState.getJSONArray("viewport").getDouble(1)-60) visibleError=true;
            }
            if(!visibleError) throw new AssertionError("Import error is outside the visible library content");
            capture("qa-document-invalid");
            chooseOwnDocument(documentFilename);
            until(15000,"private document output",data -> "local-files".equals(data.optString("current_provider")) && "Playing".equals(data.optString("playback")) && data.optJSONObject("audio_metrics")!=null && data.optJSONObject("audio_metrics").optLong("nonzero_samples")>0);
        } else {
            capture("qa-document-restored");documentControl("select:"+savedDocument(initial));
            until(15000,"restored private document output",data -> "local-files".equals(data.optString("current_provider")) && "Playing".equals(data.optString("playback")) && data.optJSONObject("audio_metrics")!=null && data.optJSONObject("audio_metrics").optLong("nonzero_samples")>0);
        }
        String id=state().getString("current");
        capture(restore?"qa-document-restored-audio":"qa-document-playing");
        if(state().getJSONObject("audio_metrics").getLong("pcm_bytes")!=65536) throw new AssertionError("Document PCM is not bounded");
        if(!restore) {
            measureResources("qa-document-resources");
            long instance=state().getLong("instance"),occurrence=state().getLong("current_occurrence");
            ActivityMonitor monitor=addMonitor(MainActivity.class.getName(),null,false);
            runOnMainSync(() -> activity.recreate());
            Activity recreated=waitForMonitorWithTimeout(monitor,5000);removeMonitor(monitor);
            if(recreated==null) throw new AssertionError("Document recreation did not return an Activity");activity=recreated;
            until(data -> data.optLong("instance")!=instance && data.optLong("current_occurrence")==occurrence && "Paused".equals(data.optString("playback")));
            capture("qa-document-recreated");tap("home");openPlayer();tap("play");
            until(data -> "Playing".equals(data.optString("playback")) && data.optJSONObject("audio_metrics")!=null && data.optJSONObject("audio_metrics").optLong("nonzero_samples")>0);
        }
        tap("pause");until(data -> "Paused".equals(data.optString("playback")));
        long position=state().getLong("position_ms");SystemClock.sleep(400);
        if(state().getLong("position_ms")!=position) throw new AssertionError("Paused document clock advances");
        slide("seek",0.5f);until(data -> data.optLong("position_ms")>55000 && data.optLong("position_ms")<65000);
        if(!restore) {
            tap("library");String saved=state().getJSONArray("saved_media").toString();int files=state().getJSONObject("local_import").getInt("files");
            chooseOwnDocument(documentFilename);
            until(data -> id.equals(data.optString("current")) && "Playing".equals(data.optString("playback")) && data.optJSONObject("audio_metrics")!=null && data.optJSONObject("audio_metrics").optLong("nonzero_samples")>0);
            if(!saved.equals(state().getJSONArray("saved_media").toString()) || state().getJSONObject("local_import").getInt("files")!=files) throw new AssertionError("Repeated document created another identity/favorite");
            tap("settings");tap("light");tap("library");capture("qa-document-library-light");
            tap("settings");tap("dark");tap("library");capture("qa-document-library-dark");
            tap("home");openPlayer();tap("pause");
        }
        JSONObject index=privateDocument("local-files-v1.json");
        for(int i=0;i<index.getJSONArray("files").length();i++) {
            String path=index.getJSONArray("files").getJSONObject(i).getString("path");
            if(!new File(path).getCanonicalPath().startsWith(new File(profileDirectory(),"imported-documents").getCanonicalPath()+"/")) throw new AssertionError("Document locator escaped private storage");
        }
        JSONObject checks=new JSONObject();checks.put("result","pass");checks.put("restored_process",restore);checks.put("id",id);checks.put("acoustic_capture",false);checks.put("state",state());
        Files.write(new File(profileDirectory(),restore?"qa-document-restore-checks.json":"qa-document-import-checks.json").toPath(),checks.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }
    private InputConnection connection(View view) {
        if(view.onCheckIsTextEditor()) { InputConnection result=view.onCreateInputConnection(new EditorInfo()); if(result!=null) return result; }
        if(view instanceof ViewGroup) { ViewGroup group=(ViewGroup)view; for(int i=0;i<group.getChildCount();i++) { InputConnection result=connection(group.getChildAt(i)); if(result!=null) return result; } }
        return null;
    }
    private void text(String text,boolean burst) throws Exception {
        editorText(text,burst,"search");until(s -> text.equals(s.optString("query")));
    }
    private void editorText(String text,boolean burst,String field) throws Exception {
        until(s -> field.equals(s.optString("editor_target")));
        final boolean[] committed={false};
        runOnMainSync(() -> {
            InputConnection input=connection(activity.getWindow().getDecorView());
            if(input!=null) {
                input.beginBatchEdit(); input.deleteSurroundingText(512,512);
                if(burst) { for(int i=0;i<text.length();i++) input.commitText(text.substring(i,i+1),1); committed[0]=true; }
                else committed[0]=input.commitText(text,1);
                input.endBatchEdit();
            }
        });
        if(!committed[0]) throw new AssertionError("Android InputConnection unavailable");
        until(s -> field.equals(s.optString("editor_target")) && text.equals(s.optString("editor_text")));
        SystemClock.sleep(500);
        if(!text.equals(state().getString("editor_text"))) throw new AssertionError("A late frame undid text input");
    }
    private JSONObject listeningHistory(JSONObject state) { return state.optJSONObject("listening_history"); }
    private void historyUi(boolean restore) throws Exception {
        final String id="26896924";
        if(!restore) {
            if(listeningHistory(state()).optInt("count")!=0) throw new AssertionError("History QA requires a fresh profile");
            tap("demo-examples");tap("select:dawn");tap("pause");slide("volume",0.08f);
            tap("search");tap("source-commons");tap("query");text("Chopin",false);hideKeyboard();
            until(25000,"audited CC0 music",state -> hasControl(state,"select:"+id));tap("select:"+id);
            until(25000,"native history source",state -> onlinePlaying(state,id) && state.optJSONObject("audio_metrics").optLong("nonzero_samples")>1000);
            tap("pause");JSONObject beforeSeek=state().getJSONObject("audio_metrics");slide("seek",0.6f);
            until(30000,"paused seek before listening",state -> "Paused".equals(state.optString("playback")) && state.optLong("position_ms")>100000);
            if(listeningHistory(state()).optInt("count")!=0) throw new AssertionError("Seeking fabricated a listen");
            tap("play");
            JSONObject counted=until(45000,"thirty seconds of delivered source frames",state -> listeningHistory(state).optInt("count")==1);
            JSONObject metrics=counted.getJSONObject("audio_metrics");
            double listenedSeconds=beforeSeek.getDouble("frames")/beforeSeek.getDouble("output_rate")+metrics.getDouble("frames")/metrics.getDouble("output_rate");
            if(listenedSeconds<30) throw new AssertionError("History counted less than thirty seconds of native output");
            if(metrics.getLong("frames")<29*metrics.getLong("output_rate")) throw new AssertionError("History counted a seek position instead of output frames");
            Files.write(new File(profileDirectory(),"qa-history-counted-frames.json").toPath(),counted.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
            tap("pause");tap("home");capture("qa-history-recent-dark");
            JSONArray link=visibleRect("history-open");if(link.getDouble(3)-link.getDouble(1)<48) throw new AssertionError("History heading touch target too small");
            tap("history-open");until(state -> "History".equals(state.optString("page")));capture("qa-history-dark");
            tap("history-clear");tap("history-clear-cancel");
            if(listeningHistory(state()).optInt("count")!=1) throw new AssertionError("Cancelling history clear changed its records");
            tap("save:"+id);until(state -> savedCount(state,id)==1);
            tap("settings");tap("light");tap("home");capture("qa-history-recent-light");tap("history-open");capture("qa-history-light");
            long instance=state().getLong("instance");
            ActivityMonitor monitor=addMonitor(MainActivity.class.getName(),null,false);runOnMainSync(() -> activity.recreate());
            Activity recreated=waitForMonitorWithTimeout(monitor,8000);removeMonitor(monitor);
            if(recreated==null) throw new AssertionError("History recreation did not return");activity=recreated;
            until(state -> state.optLong("instance")!=instance && listeningHistory(state).optInt("count")==1);
            settledForeground();capture("qa-history-recreated");
        } else {
            if(!state().getString("query").isEmpty() || state().getJSONObject("online").getBoolean("worker_initialized") || listeningHistory(state()).getInt("count")!=1) throw new AssertionError("History restoration required search or provider IO");
            capture("qa-history-restored");tap("history-play:"+id);
            until(25000,"native replay from restored history without search",state -> onlinePlaying(state,id) && state.optJSONObject("audio_metrics").optLong("nonzero_samples")>1000);
            if(!state().getString("query").isEmpty()) throw new AssertionError("History replay performed a text search");
            tap("pause");tap("home");tap("history-open");
            JSONObject before=state();long occurrence=before.getLong("current_occurrence");
            String context=before.getJSONArray("context_occurrences").toString(),manual=before.getJSONArray("manual_occurrences").toString(),saved=before.getJSONArray("saved_media").toString();
            tap("history-clear");capture("qa-history-clear-confirmation");tap("history-clear-confirm");until(state -> listeningHistory(state).optInt("count")==0);
            JSONObject after=state();
            if(after.getLong("current_occurrence")!=occurrence || !context.equals(after.getJSONArray("context_occurrences").toString()) || !manual.equals(after.getJSONArray("manual_occurrences").toString()) || !saved.equals(after.getJSONArray("saved_media").toString())) throw new AssertionError("Clearing history changed queue/favorites");
            capture("qa-history-cleared");
        }
        Files.write(new File(profileDirectory(),restore?"qa-history-restore-checks.json":"qa-history-checks.json").toPath(),new JSONObject().put("result","pass").put("physical_device",true).put("id",id).put("acoustic_capture",false).put("restored_process",restore).put("state",state()).toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }
    private boolean catalogLoaded(JSONObject state) {
        JSONObject catalog=state.optJSONObject("music_catalog");
        return catalog!=null && !catalog.optBoolean("waiting") && catalog.optInt("count",0)>0 && catalog.isNull("error");
    }
    private boolean savedCatalog(JSONObject state,String id) {
        JSONArray items=state.optJSONArray("saved_media");
        if(items!=null) for(int i=0;i<items.length();i++) {
            JSONObject item=items.optJSONObject(i);
            if("musicbrainz".equals(item.optString("provider")) && id.equals(item.optString("id"))) return true;
        }
        return false;
    }
    private boolean catalogLibraryDurable(int count) {
        try {return privateDocument("library-v1.json").getJSONArray("items").length()==count;}
        catch(Exception error) {return false;}
    }
    private void catalogLibraryUi(boolean restore) throws Exception {
        final String artist="artist:b7ffd2af-418f-4be2-bdd1-22f8b48613da",album="release:12b57d46-a192-499e-a91f-7da66790a1c1";
        if(!restore) {
            if(state().getJSONArray("saved_media").length()!=0) throw new AssertionError("Catalog library QA requires a fresh profile");
            tap("search");tap("source-musicbrainz");tap("catalog-kind:Release");tap("query");text("The Slip",false);hideKeyboard();
            until(25000,"identified album",state -> hasControl(state,"catalog-open:"+album));tap("catalog-open:"+album);until(25000,"album tracks",this::catalogLoaded);
            tap("catalog-save:"+album);until(state -> savedCatalog(state,album));
            tap("catalog-artist:"+artist);until(25000,"album artist",state -> catalogLoaded(state) && artist.equals(state.optJSONObject("music_catalog").optString("detail")));
            tap("catalog-save:"+artist);until(state -> savedCatalog(state,artist));capture("qa-saved-artist");
            tap("library");capture("qa-saved-catalog-dark");tap("settings");tap("light");tap("library");capture("qa-saved-catalog-light");
            until(state -> savedCatalog(state,artist) && savedCatalog(state,album) && catalogLibraryDurable(2));
        } else {
            JSONObject first=state();
            if(!savedCatalog(first,artist) || !savedCatalog(first,album) || first.getJSONArray("saved_media").length()!=2 || !first.getString("query").isEmpty() || first.getJSONObject("online").getBoolean("worker_initialized") || first.getJSONObject("music_catalog").getLong("generation")!=0) throw new AssertionError("Catalog restoration needed network or lost saved entity types");
            tap("library");capture("qa-saved-catalog-restored");tap("select:"+artist);
            until(25000,"restored artist recordings",state -> catalogLoaded(state) && artist.equals(state.optJSONObject("music_catalog").optString("detail")));capture("qa-saved-artist-reopened");
            tap("library");tap("select:"+album);until(25000,"restored album tracks and credits",state -> catalogLoaded(state) && album.equals(state.optJSONObject("music_catalog").optString("detail")) && hasControl(state,"catalog-artist:"+artist));capture("qa-saved-album-reopened");
            tap("catalog-save:"+album);until(state -> !savedCatalog(state,album) && savedCatalog(state,artist) && catalogLibraryDurable(1));tap("library");capture("qa-saved-album-removed");
            if(!state().getString("query").isEmpty()) throw new AssertionError("Saved entity reopening issued a title search");
        }
        JSONObject after=state();
        if(after.optBoolean("audio") || !after.isNull("audio_metrics") || after.getJSONArray("context_occurrences").length()!=0 || after.getJSONArray("manual_occurrences").length()!=0) throw new AssertionError("Saving or opening catalog metadata changed the audio queue");
        Files.write(new File(profileDirectory(),restore?"qa-saved-catalog-restore-checks.json":"qa-saved-catalog-checks.json").toPath(),new JSONObject().put("result","pass").put("artist_id",artist).put("release_id",album).put("restored_process",restore).put("saved_types",true).put("queue_preserved",true).put("audio",false).toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }
    private void artistReleasesUi() throws Exception {
        final String artist="artist:b7ffd2af-418f-4be2-bdd1-22f8b48613da";
        tap("search");tap("source-musicbrainz");tap("catalog-kind:Artist");tap("query");text("Nine Inch Nails",false);hideKeyboard();
        until(25000,"identified artist",state -> hasControl(state,"catalog-open:"+artist));tap("catalog-open:"+artist);
        until(25000,"artist recordings",this::catalogLoaded);capture("qa-artist-recordings");
        tap("artist-releases");JSONObject releases=until(25000,"artist editions",state -> catalogLoaded(state) && state.optJSONObject("music_catalog").optBoolean("artist_releases"));
        if(!artist.equals(releases.getJSONObject("music_catalog").getString("detail"))) throw new AssertionError("Edition browse changed artist identity");
        JSONArray target=visibleRect("artist-releases");if(target.getDouble(3)-target.getDouble(1)<48) throw new AssertionError("Artist tab touch target too small");
        capture("qa-artist-editions-dark");
        final int count=releases.getJSONObject("music_catalog").getInt("count");
        tap("catalog-next");until(25000,"next editions page",state -> catalogLoaded(state) && state.optJSONObject("music_catalog").optInt("offset")==count);capture("qa-artist-editions-next");
        tap("catalog-previous");JSONObject first=until(25000,"previous editions page",state -> catalogLoaded(state) && state.optJSONObject("music_catalog").optInt("offset")==0);
        String chosen=null;JSONArray controls=first.getJSONArray("controls");
        for(int i=0;i<controls.length();i++) {String name=controls.getJSONObject(i).getString("name");if(name.startsWith("catalog-open:release:")) {chosen=name;break;}}
        if(chosen==null) throw new AssertionError("No identified edition to open");
        final String edition=chosen.substring("catalog-open:".length());tap(chosen);
        until(25000,"edition tracks",state -> catalogLoaded(state) && edition.equals(state.optJSONObject("music_catalog").optString("detail")));capture("qa-artist-album-tracks");
        tap("catalog-back");until(state -> catalogLoaded(state) && artist.equals(state.optJSONObject("music_catalog").optString("detail")) && state.optJSONObject("music_catalog").optBoolean("artist_releases"));
        tap("settings");tap("light");tap("search");capture("qa-artist-editions-light");
        tap("artist-recordings");until(25000,"return to recordings",state -> catalogLoaded(state) && !state.optJSONObject("music_catalog").optBoolean("artist_releases"));
        if(state().optBoolean("audio") || !state().isNull("audio_metrics")) throw new AssertionError("Metadata browsing started audio");
        Files.write(new File(profileDirectory(),"qa-artist-checks.json").toPath(),new JSONObject().put("result","pass").put("artist_id",artist).put("release_id",edition).put("first_page_count",count).put("paging_and_back",true).put("album_tracks",true).put("album_audio",false).toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }
    private void artworkUi() throws Exception {
        final String release="release:12b57d46-a192-499e-a91f-7da66790a1c1";
        tap("search");tap("source-musicbrainz");tap("catalog-kind:Release");tap("query");text("The Slip",false);hideKeyboard();
        until(25000,"exact licensed album edition",state -> hasControl(state,"catalog-open:"+release));
        capture("qa-artwork-results");tap("catalog-open:"+release);
        JSONObject loaded=until(25000,"release artwork decoded",state -> {
            JSONObject art=state.optJSONObject("music_catalog").optJSONObject("artwork");
            return art!=null && release.equals(art.optString("id")) && art.optJSONArray("dimensions")!=null;
        });
        JSONArray dimensions=loaded.getJSONObject("music_catalog").getJSONObject("artwork").getJSONArray("dimensions");
        if(dimensions.getInt(0)>250 || dimensions.getInt(1)>250 || !loaded.isNull("current") || !loaded.isNull("audio_metrics")) throw new AssertionError("Artwork identity/limits or unexpected album playback");
        SystemClock.sleep(800);capture("qa-artwork-dark");
        tap("settings");tap("light");tap("search");
        until(state -> state.optJSONObject("music_catalog").optJSONObject("artwork").optBoolean("cache_hit"));capture("qa-artwork-light");
        tap("catalog-back");until(state -> state.optJSONObject("music_catalog").isNull("detail"));
        tap("catalog-open:"+release);until(state -> state.optJSONObject("music_catalog").optJSONObject("artwork").optBoolean("cache_hit"));
        long instance=state().getLong("instance");
        ActivityMonitor monitor=addMonitor(MainActivity.class.getName(),null,false);runOnMainSync(() -> activity.recreate());
        Activity recreated=waitForMonitorWithTimeout(monitor,8000);removeMonitor(monitor);
        if(recreated==null) throw new AssertionError("Artwork Activity recreation did not return");activity=recreated;
        until(state -> state.optLong("instance")!=instance && state.optJSONObject("music_catalog").optJSONObject("artwork").optBoolean("cache_hit"));
        settledForeground();SystemClock.sleep(800);capture("qa-artwork-recreated");
        Files.write(new File(profileDirectory(),"qa-artwork-checks.json").toPath(),new JSONObject()
            .put("result","pass").put("release",release).put("dimensions",dimensions).put("cached_reopen",true)
            .put("activity_recreation",true).put("album_audio",false)
            .put("artwork_credit","The Slip — art direction Rob Sheridan and Trent Reznor")
            .put("artwork_license","CC BY-NC-SA 3.0 US; thumbnail resized for private noncommercial UI QA")
            .put("license_url","https://creativecommons.org/licenses/by-nc-sa/3.0/us/")
            .put("license_evidence","https://wiki.creativecommons.org/wiki/Nine_Inch_Nails_The_Slip")
            .toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }
    private void musicUi() throws Exception {
        if(hasControl(state(),"player") || hasControl(state(),"play") || !state().isNull("current")) throw new AssertionError("Fresh browsing screen must not show an empty mini-player");
        capture("qa-mobile-empty");
        // Adjust application gain on the existing silent demo, before any audio.
        tap("demo-examples");tap("select:dawn");tap("pause");slide("volume",0.08f);
        until(s -> s.optDouble("volume")>0.02 && s.optDouble("volume")<0.15);
        tap("search");tap("source-commons");tap("query");text(musicQuery,true);
        capture("qa-mobile-ime");hideKeyboard();
        until(20000,"selected CC0 music result",s -> hasControl(s,"select:"+musicId) && "Ready".equals(s.optJSONObject("online").optString("search_status")));
        tap("save:"+musicId);until(s -> savedCount(s,musicId)==1);tap("save-again:"+musicId);
        until(s -> savedCount(s,musicId)==1);capture("qa-mobile-results-dark");
        tap("settings");tap("light");tap("search");capture("qa-mobile-results-light");
        tap("settings");tap("dark");tap("search");tap("select:"+musicId,0);
        JSONObject first=until(25000,"progressive music delivered to native callback",s -> onlinePlaying(s,musicId) && s.optJSONObject("audio_metrics").optLong("nonzero_samples")>1000);
        JSONObject metrics=first.getJSONObject("audio_metrics"),network=first.getJSONObject("online").getJSONObject("network");
        if(!metrics.getBoolean("streaming") || metrics.getLong("pcm_bytes")>65536 || network.getLong("received_bytes")<=0 || network.getLong("received_bytes")>=network.getLong("total_bytes")) throw new AssertionError("Music must produce bounded PCM before the complete encoded file arrives");
        Files.write(new File(profileDirectory(),"qa-music-first-pcm.json").toPath(),first.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
        capture("qa-mobile-playing");tap("pause");until(s -> "Paused".equals(s.optString("playback")));SystemClock.sleep(300);
        long paused=state().getLong("position_ms");SystemClock.sleep(400);
        if(state().getLong("position_ms")!=paused) throw new AssertionError("Paused stream clock advanced");
        slide("seek",0.5f);until(30000,"paused HTTP seek",s -> "Paused".equals(s.optString("playback")) && s.optLong("position_ms")>=musicDurationMs/2-1500 && s.optLong("position_ms")<=musicDurationMs/2+1500);
        capture("qa-mobile-seek");tap("play");until(30000,"native resume after HTTP seek",s -> onlinePlaying(s,musicId) && s.optLong("position_ms")>musicDurationMs/2);tap("pause");
        tap("search");until(s -> hasControl(s,"player") && hasControl(s,"play") && hasControl(s,"next"));capture("qa-mobile-mini");
        tap("player");until(s -> "Player".equals(s.optString("page")));
        sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_BACK);until(s -> "Search".equals(s.optString("page")));
        tap("library");capture("qa-mobile-library");tap("select:"+musicId);until(s -> "Player".equals(s.optString("page")));tap("stop");until(s -> "Paused".equals(s.optString("playback")));
        sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_BACK);until(s -> "Library".equals(s.optString("page")));
        tap("player");long instance=state().getLong("instance"),occurrence=state().getLong("current_occurrence");
        ActivityMonitor monitor=addMonitor(MainActivity.class.getName(),null,false);runOnMainSync(() -> activity.recreate());
        Activity recreated=waitForMonitorWithTimeout(monitor,8000);removeMonitor(monitor);
        if(recreated==null) throw new AssertionError("Mobile player recreation did not return");activity=recreated;
        until(s -> s.optLong("instance")!=instance && s.optLong("current_occurrence")==occurrence && "Paused".equals(s.optString("playback")) && s.isNull("audio_metrics"));
        settledForeground();capture("qa-mobile-recreated");
        int original=activity.getRequestedOrientation();
        runOnMainSync(() -> activity.setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE));
        until(s -> s.optJSONArray("viewport").optDouble(0)>s.optJSONArray("viewport").optDouble(1));
        visibleRect("volume");
        JSONObject landscape=state();JSONArray page=landscape.getJSONArray("page_rect");
        for(String name:new String[]{"volume","player-background-notice"}) {
            JSONArray rect=controlRect(landscape,name);
            if(rect==null || rect.getDouble(1)<page.getDouble(1) || rect.getDouble(3)>page.getDouble(3)) throw new AssertionError("Landscape content clipped: "+name);
        }
        capture("qa-mobile-landscape");
        runOnMainSync(() -> activity.setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_PORTRAIT));
        until(s -> s.optJSONArray("viewport").optDouble(0)<s.optJSONArray("viewport").optDouble(1));settledForeground();
        tap("stop");until(s -> "Paused".equals(s.optString("playback")) && s.isNull("audio_metrics") && s.optJSONObject("online").isNull("network"));
        SystemClock.sleep(state().getJSONObject("online").optLong("request_timeout_ms",2000)+300);String retired=state().getJSONObject("online").optString("retired_network");SystemClock.sleep(500);
        if(!retired.equals(state().getJSONObject("online").optString("retired_network"))) throw new AssertionError("Cancelled music continued network work");
        runOnMainSync(() -> activity.setRequestedOrientation(original));capture("qa-mobile-cancelled");
        JSONObject checks=new JSONObject().put("result","pass").put("acoustic_capture",false).put("first_pcm_received_bytes",network.getLong("received_bytes")).put("total_bytes",network.getLong("total_bytes")).put("pcm_bytes",metrics.getLong("pcm_bytes")).put("private_profile",profile).put("selected_id",musicId).put("query",musicQuery).put("expected_duration_ms",musicDurationMs).put("checks",new JSONArray(java.util.Arrays.asList("conditional mini-player","IME query","dark and light","real CC0 music search","idempotent save","native PCM before whole file","paused clock","HTTP seek","resume","mini-player controls","Android Back to origin","Activity recreation","landscape reachable controls","cancel stops output and transport")));
        Files.write(new File(profileDirectory(),"qa-mobile-checks.json").toPath(),checks.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }
    private void queueUi() throws Exception {
        tap("library");
        JSONObject before=state();long current=before.getLong("current_occurrence");String context=before.getJSONArray("context_occurrences").toString(),saved=before.getJSONArray("saved_media").toString();
        if(before.getJSONArray("manual_occurrences").length()!=0) throw new AssertionError("Queue trace requires an empty manual prefix");
        tap("add:"+musicId);tap("add:"+musicId);until(s -> s.optJSONArray("manual_occurrences").length()==2);
        long first=state().getJSONArray("manual_occurrences").getLong(0),second=state().getJSONArray("manual_occurrences").getLong(1);
        tap("queue");capture("qa-queue-dark");tap("queue-more:"+first);capture("qa-queue-menu");
        for(String name:new String[]{"down:0","remove:0"}) {
            JSONArray rect=visibleRect(name);
            if(rect.getDouble(2)-rect.getDouble(0)<48 || rect.getDouble(3)-rect.getDouble(1)<48) throw new AssertionError("Queue menu touch target too small: "+name);
        }
        tap("down:0");until(s -> s.optJSONArray("manual_occurrences").optLong(0)==second && s.optJSONArray("manual_occurrences").optLong(1)==first);
        tap("up:1");until(s -> s.optJSONArray("manual_occurrences").optLong(0)==first && s.optJSONArray("manual_occurrences").optLong(1)==second);
        tap("settings");tap("light");tap("queue");capture("qa-queue-light");tap("settings");tap("dark");tap("queue");
        tap("remove:1");until(s -> s.optJSONArray("manual_occurrences").length()==1 && s.optJSONArray("manual_occurrences").optLong(0)==first);
        tap("clear");until(s -> s.optJSONArray("manual_occurrences").length()==0);capture("qa-queue-cleared");
        JSONObject after=state();
        if(after.getLong("current_occurrence")!=current || !context.equals(after.getJSONArray("context_occurrences").toString()) || !saved.equals(after.getJSONArray("saved_media").toString())) throw new AssertionError("Manual queue editing changed current/context/favorites");
        tap("queue-current");until(s -> "Player".equals(s.optString("page")));sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_BACK);until(s -> "Queue".equals(s.optString("page")));
        Files.write(new File(profileDirectory(),"qa-queue-checks.json").toPath(),new JSONObject().put("result","pass").put("duplicate_identity",true).put("current_context_saved_preserved",true).put("menu_targets_at_least_48",true).put("back_to_queue",true).toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }
    private void playlistUi() throws Exception {
        tap("library");tap("playlist-name");editorText("Café nocturno",false,"playlist-name");hideKeyboard();tap("playlist-create");
        until(s -> "Playlist".equals(s.optString("page")) && s.optJSONArray("playlists").length()==1);
        long id=state().getJSONArray("playlists").getJSONObject(0).getLong("id");capture("qa-playlist-empty");
        tap("library");
        long occurrence=state().getLong("current_occurrence");String context=state().getJSONArray("context_occurrences").toString();
        tap("playlist-add:"+id+":"+musicId);until(s -> s.optJSONArray("playlists").optJSONObject(0).optJSONArray("entries").length()==1);
        tap("playlist-add:"+id+":"+musicId);until(s -> s.optJSONArray("playlists").optJSONObject(0).optJSONArray("entries").length()==2);
        if(state().getLong("current_occurrence")!=occurrence || !context.equals(state().getJSONArray("context_occurrences").toString())) throw new AssertionError("Adding to a playlist changed the queue");
        tap("playlist-open:"+id);tap("playlist-select:1");
        JSONObject playing=until(25000,"playlist music at the selected duplicate",s -> onlinePlaying(s,musicId) && s.optJSONObject("audio_metrics").optLong("nonzero_samples")>1000);
        if(playing.getLong("current_occurrence")!=playing.getJSONArray("context_occurrences").getLong(1)) throw new AssertionError("Selected duplicate opened the first occurrence");
        capture("qa-playlist-playing");tap("pause");until(s -> "Paused".equals(s.optString("playback")));
        tap("library");tap("playlist-open:"+id);
        occurrence=state().getLong("current_occurrence");context=state().getJSONArray("context_occurrences").toString();
        tap("playlist-up:1");tap("playlist-remove:0");until(s -> s.optJSONArray("playlists").optJSONObject(0).optJSONArray("entries").length()==1);
        tap("playlist-edit");tap("playlist-rename");editorText("Café · lectura",false,"playlist-rename");hideKeyboard();tap("playlist-rename-save");
        until(s -> "Café · lectura".equals(s.optJSONArray("playlists").optJSONObject(0).optString("name")));
        if(!musicQuery.equals(state().getString("query"))) throw new AssertionError("Playlist input changed the catalog query");
        tap("playlist-delete");tap("playlist-delete-cancel");
        if(state().getJSONArray("playlists").length()!=1) throw new AssertionError("Cancelling deletion removed the playlist");
        if(state().getLong("current_occurrence")!=occurrence || !context.equals(state().getJSONArray("context_occurrences").toString())) throw new AssertionError("Editing a playlist changed the queue");
        capture("qa-playlist-edited-dark");tap("settings");tap("light");tap("library");tap("playlist-open:"+id);capture("qa-playlist-edited-light");
        tap("settings");tap("dark");tap("library");tap("playlist-open:"+id);
        until(s -> privatePlaylistSaved(id));
        Files.write(new File(profileDirectory(),"qa-playlist-checks.json").toPath(),new JSONObject().put("result","pass").put("id",id).put("duplicate_selection",true).put("queue_preserved",true).put("unicode_ime",true).put("durable",true).put("deletion_cancelled",true).toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }
    private boolean privatePlaylistSaved(long id) {
        try {
            JSONObject list=privateDocument("playlists-v1.json").getJSONArray("lists").getJSONObject(0);
            return list.getLong("id")==id && "Café · lectura".equals(list.getString("name")) && list.getJSONArray("entries").length()==1;
        } catch(Exception error) {return false;}
    }
    private void youtubeSessionStore(boolean restore) throws Exception {
        // A nested synthetic profile cannot be restored by the running Rust UI.
        File directory=new File(profileDirectory(),"session-store-fixture");
        String path=directory.getAbsolutePath();
        byte[] fixture="SAPISID=synthetic-android-store-fixture".getBytes(java.nio.charset.StandardCharsets.UTF_8);
        File file=new File(directory,"youtube-session-v1.bin");
        if(!restore) {
            if(YoutubeSessionStore.load(getTargetContext(),path)!=null) throw new AssertionError("Use a fresh session-store fixture");
            YoutubeSessionStore.store(getTargetContext(),path,fixture);
            byte[] sealed=Files.readAllBytes(file.toPath());
            if(new String(sealed,java.nio.charset.StandardCharsets.ISO_8859_1).contains("SAPISID=")) throw new AssertionError("Session was stored without encryption");
            if(!java.util.Arrays.equals(fixture,YoutubeSessionStore.load(getTargetContext(),path))) throw new AssertionError("Native session store round trip failed");
            YoutubeSessionStore.store(getTargetContext(),path,fixture);
            if(java.util.Arrays.equals(sealed,Files.readAllBytes(file.toPath()))) throw new AssertionError("Session nonce was reused");
        } else {
            if(!java.util.Arrays.equals(fixture,YoutubeSessionStore.load(getTargetContext(),path))) throw new AssertionError("Native session store did not survive process restart");
            byte[] sealed=Files.readAllBytes(file.toPath());sealed[sealed.length-1]^=1;Files.write(file.toPath(),sealed);
            boolean rejected=false;
            try {YoutubeSessionStore.load(getTargetContext(),path);} catch(java.io.IOException expected) {rejected=true;}
            if(!rejected) throw new AssertionError("Tampered session was accepted");
            byte[] future=sealed.clone();future[3]=2;Files.write(file.toPath(),future);
            rejected=false;
            try {YoutubeSessionStore.store(getTargetContext(),path,fixture);} catch(java.io.IOException expected) {rejected=true;}
            if(!rejected || !java.util.Arrays.equals(future,Files.readAllBytes(file.toPath()))) throw new AssertionError("Future session storage was overwritten");
            YoutubeSessionStore.forget(getTargetContext(),path);
            if(YoutubeSessionStore.load(getTargetContext(),path)!=null || file.exists()) throw new AssertionError("Sign-out retained session material");
        }
        Files.write(new File(profileDirectory(),restore?"qa-session-store-restored.json":"qa-session-store-written.json").toPath(),
            new JSONObject().put("result","pass").put("pid",android.os.Process.myPid()).put("synthetic",true).put("encrypted",true).put("restored",restore).put("tamper_rejected",restore).put("forgotten",restore).toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }
    private void youtubeAuthPolicy() throws Exception {
        for(String url:new String[]{"https://accounts.google.com/ServiceLogin","https://music.youtube.com/","https://consent.youtube.com/"})
            if(!YoutubeAuthActivity.allowed(url)) throw new AssertionError("Expected authentication origin rejected");
        for(String url:new String[]{"http://accounts.google.com/","https://accounts.google.com.attacker.invalid/","https://accounts.google.com:443/","https://user@accounts.google.com/","file:///data/","intent://login/","https://www.google.com/search"})
            if(YoutubeAuthActivity.allowed(url)) throw new AssertionError("Unexpected authentication navigation allowed");
        if(!YoutubeAuthActivity.denied("https://accounts.google.com/v3/signin/rejected?error=disallowed_useragent") || YoutubeAuthActivity.denied("https://music.youtube.com/"))
            throw new AssertionError("Provider refusal was not distinguished");
        if(YoutubeAuthActivity.sessionReady(null) || YoutubeAuthActivity.sessionReady("SAPISID=anonymous") || YoutubeAuthActivity.sessionReady("LOGIN_INFO=marker-only") || YoutubeAuthActivity.sessionReady("LOGIN_INFO=; SAPISID=fixture"))
            throw new AssertionError("Incomplete service cookies ended authentication");
        if(!YoutubeAuthActivity.sessionReady("LOGIN_INFO=fixture; SAPISID=synthetic") || !YoutubeAuthActivity.sessionReady("LOGIN_INFO=fixture; __Secure-3PAPISID=synthetic"))
            throw new AssertionError("Complete service cookies require an unrelated page navigation");
        android.content.pm.ActivityInfo info=getTargetContext().getPackageManager().getActivityInfo(new android.content.ComponentName(getTargetContext(),YoutubeAuthActivity.class),0);
        if(info.exported || !info.processName.endsWith(":youtube_auth")) throw new AssertionError("Authentication host is not isolated");
        Files.write(new File(profileDirectory(),"qa-youtube-auth-policy.json").toPath(),new JSONObject().put("result","pass").put("separate_process",true).put("exported",false).put("origins_checked",10).put("cookie_capture_guards",6).toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }
    @Override public void onStart() {
        Bundle result=new Bundle();
        try {
            Intent intent=new Intent(getTargetContext(),MainActivity.class).putExtra("dev.eirom.doremi.qa.profile",profile).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            if("document-import".equals(mode)) intent.putExtra("dev.eirom.doremi.qa.documentFolder",documentFolder);
            activity=startActivitySync(intent);
            until(s -> s.optString("page").equals("Home"));
            // Diagnostics are published before GL finishes painting. PixelCopy
            // requires a real first swap before injecting the first touch.
            capture("qa-startup");
            if("youtube-auth-policy".equals(mode)) {
                youtubeAuthPolicy();
                result.putString("stream","PASS: isolated YouTube authentication host and allowed navigation; no real account authorization\n");finish(Activity.RESULT_OK,result);return;
            }
            if("session-store-write".equals(mode) || "session-store-restore".equals(mode)) {
                youtubeSessionStore("session-store-restore".equals(mode));
                result.putString("stream","PASS: synthetic Android Keystore session storage; no real account or network authorization\n");finish(Activity.RESULT_OK,result);return;
            }
            if("catalog-library".equals(mode) || "catalog-library-restore".equals(mode)) {
                catalogLibraryUi("catalog-library-restore".equals(mode));result.putString("stream","PASS: saved artist/album types, process restoration, reopening by ID and queue preservation; no audio\n");finish(Activity.RESULT_OK,result);return;
            }
            if("artist-releases".equals(mode)) {
                artistReleasesUi();result.putString("stream","PASS: real artist editions, pagination, album tracks, back navigation and themes; no album audio\n");finish(Activity.RESULT_OK,result);return;
            }
            if("history-ui".equals(mode) || "history-restore".equals(mode)) {
                historyUi("history-restore".equals(mode));result.putString("stream","PASS: actual native listening history, shared recent cards, seek exclusion and persistence\n");finish(Activity.RESULT_OK,result);return;
            }
            if("artwork-ui".equals(mode)) {
                artworkUi();result.putString("stream","PASS: exact release artwork, cache, themes and Activity recreation; no album audio\n");
                finish(Activity.RESULT_OK,result);return;
            }
            if("music-ui".equals(mode) || "music-playlists".equals(mode)) {
                musicUi();
                if("music-playlists".equals(mode)) {queueUi();playlistUi();}result.putString("stream","PASS: private-profile mobile UI and progressive CC0 music / native callback / Android Back / lifecycle / no acoustic capture\n");finish(Activity.RESULT_OK,result);return;
            }
            if("document-import".equals(mode) || "document-restore".equals(mode)) {
                documentImport("document-restore".equals(mode));
                result.putString("stream","PASS: explicitly selected private audio / real bounded native output / controls / identity / original favorites preserved\n");
                finish(Activity.RESULT_OK,result);return;
            }
            if("library-restore".equals(mode)) {
                restoredLibrary();
                result.putString("stream","PASS: saved-library process restoration without automatic IO, qualified-ID playback without search and positive native output\n");
                finish(Activity.RESULT_OK,result);return;
            }
            // Existing development data is retained. Normalize modes through controls.
            tap("search");tap("source-demo");tap("cancel-search");
            if(state().optBoolean("shuffle")) tap("shuffle");
            while(!state().optString("repeat").equals("Off")) tap("repeat");
            tap("home");
            tap("add:dawn");tap("add:dawn");tap("select:rain");
            until(s -> s.optString("current").equals("rain") && s.optJSONArray("manual").length()==2);
            tap("search");tap("query");
            text("Lluvia",true);text("Café",false);
            runOnMainSync(() -> ((android.view.inputmethod.InputMethodManager)activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(activity.getWindow().getDecorView().getWindowToken(),0));
            SystemClock.sleep(600);
            capture("qa-unicode");
            JSONObject before=state(); long instance=before.getLong("instance"); String manual=before.getJSONArray("manual").toString();
            ActivityMonitor monitor=addMonitor(MainActivity.class.getName(),null,false);
            runOnMainSync(() -> activity.recreate());
            Activity recreated=waitForMonitorWithTimeout(monitor,8000);removeMonitor(monitor);
            if(recreated==null) throw new AssertionError("Recreated Activity did not resume");
            activity=recreated;
            JSONObject after=until(s -> s.optLong("instance")>instance);
            if(!"Café".equals(after.getString("query")) || !"rain".equals(after.getString("current")) || !manual.equals(after.getJSONArray("manual").toString())) throw new AssertionError("Activity recreation lost text or queue occurrences");
            capture("qa-recreated");
            int originalOrientation=activity.getRequestedOrientation();
            runOnMainSync(() -> activity.setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE));
            until(s -> s.optJSONArray("viewport").optDouble(0)>s.optJSONArray("viewport").optDouble(1));
            SystemClock.sleep(400);capture("qa-landscape");
            runOnMainSync(() -> activity.setRequestedOrientation(ActivityInfo.SCREEN_ORIENTATION_PORTRAIT));
            until(s -> s.optJSONArray("viewport").optDouble(0)<s.optJSONArray("viewport").optDouble(1));
            SystemClock.sleep(400);capture("qa-portrait");
            runOnMainSync(() -> activity.setRequestedOrientation(originalOrientation));
            capture("qa-rotation-return");
            tap("cancel-search");until(s -> s.optString("query").isEmpty());
            tap("next");until(s -> s.optString("current").equals("dawn") && s.optJSONArray("manual").length()==1);
            tap("next");until(s -> s.optString("current").equals("dawn") && s.optJSONArray("manual").length()==0);
            tap("next");until(s -> s.optString("current").equals("cafe"));
            tap("pause");until(s -> s.optString("playback").equals("Paused"));
            capture("qa-shared-queue-trace");
            backgroundInput();
            audio();
            longAudio();
            online();
            savedLibrary();
            result.putString("stream","PASS: Unicode/burst IME, Activity recreation with duplicate queue, landscape/portrait, shared rain -> dawn -> dawn -> cafe trace; IME cancel after background; native PCM play/pause/seek/volume/next/previous/EOF/cancel/focus/recreation/background; long WAV bounded delivery/positive samples/seek/EOF/cancel; real Commons search/load/cancel/PCM/next/previous/paused recreation/background; qualified favorites/save/remove/durable metadata/legacy preservation\n");
            finish(Activity.RESULT_OK,result);
        } catch(Throwable error) {
            try {
                capture("qa-failure");
                lastPointer.put("last_action",lastAction);
                Files.write(new File(profileDirectory(),"qa-input-delivery.json").toPath(),lastPointer.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
            } catch(Throwable diagnosticError) { error.addSuppressed(diagnosticError); }
            result.putString("stream","FAIL: "+lastAction+"\n"+android.util.Log.getStackTraceString(error)+"\n");finish(Activity.RESULT_CANCELED,result); }
    }
}
