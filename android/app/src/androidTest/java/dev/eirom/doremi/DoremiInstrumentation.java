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
    private String mode="full";
    private String documentFolder="";
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); mode=arguments==null?"full":arguments.getString("mode","full"); documentFolder=arguments==null?"":arguments.getString("documentFolder",""); start(); }
    private JSONObject state() throws Exception {
        return new JSONObject(new String(Files.readAllBytes(new File(getTargetContext().getFilesDir(),"qa-state.json").toPath()),java.nio.charset.StandardCharsets.UTF_8));
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
            final boolean[] focused={false};
            runOnMainSync(() -> focused[0]=!activity.isFinishing() && !activity.isDestroyed() && activity.hasWindowFocus() && activity.getWindow().getDecorView().isShown());
            if(focused[0]) {waitForIdleSync();return;}
            SystemClock.sleep(80);
        } while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Doremi did not regain window focus; no input injected");
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
                try(FileOutputStream file=new FileOutputStream(new File(getTargetContext().getFilesDir(),name+".png"))) {image[0].compress(Bitmap.CompressFormat.PNG,100,file);}
                image[0].recycle();
                JSONObject snapshot=state();
                snapshot.put("qa_surface_origin",new JSONArray().put(surfaceOrigin[0]).put(surfaceOrigin[1]));
                snapshot.put("qa_content_origin",new JSONArray().put(contentOrigin[0]).put(contentOrigin[1]));
                Files.write(new File(getTargetContext().getFilesDir(),name+".json").toPath(),snapshot.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
                return;
            }
            if(image[0]!=null) image[0].recycle();
            if(lastStatus!=PixelCopy.ERROR_SOURCE_NO_DATA && lastStatus!=PixelCopy.ERROR_TIMEOUT) break;
            SystemClock.sleep(120);
        } while(SystemClock.uptimeMillis()<deadline);
        throw new AssertionError("Doremi surface capture failed: "+name+" status="+lastStatus);
    }
    private void tap(String name) throws Exception { tap(name,500); }
    private void tap(String name,long settle) throws Exception {
        lastAction="tap:"+name;
        foreground();
        JSONObject data=state(); JSONArray controls=data.getJSONArray("controls"); JSONArray rect=null;
        for(int i=0;i<controls.length();i++) { JSONObject control=controls.getJSONObject(i); if(name.equals(control.getString("name"))) { rect=control.getJSONArray("rect"); break; } }
        if(rect==null) throw new AssertionError("Missing control: "+name);
        final int[] origin=new int[2];
        runOnMainSync(() -> surface(activity.getWindow().getDecorView()).getLocationOnScreen(origin));
        float scale=(float)data.getDouble("pixels_per_point");
        float x=origin[0]+(float)(rect.getDouble(0)+rect.getDouble(2))*scale/2;
        float y=origin[1]+(float)(rect.getDouble(1)+rect.getDouble(3))*scale/2;
        long time=SystemClock.uptimeMillis();
        MotionEvent down=MotionEvent.obtain(time,time,MotionEvent.ACTION_DOWN,x,y,0); sendPointerSync(down); down.recycle();
        // Hold across rendered frames; a future event timestamp alone does not
        // keep the native pointer down while egui evaluates slider interaction.
        SystemClock.sleep(80);
        MotionEvent up=MotionEvent.obtain(time,SystemClock.uptimeMillis(),MotionEvent.ACTION_UP,x,y,0); sendPointerSync(up); up.recycle();
        SystemClock.sleep(settle);
    }
    private void slide(String name,float fraction) throws Exception {
        lastAction="slide:"+name+":"+fraction;
        foreground();
        JSONObject data=state(); JSONArray controls=data.getJSONArray("controls");JSONArray rect=null;
        for(int i=0;i<controls.length();i++) {JSONObject c=controls.getJSONObject(i);if(name.equals(c.getString("name"))) rect=c.getJSONArray("rect");}
        if(rect==null) throw new AssertionError("Missing slider: "+name);
        final int[] origin=new int[2];runOnMainSync(() -> surface(activity.getWindow().getDecorView()).getLocationOnScreen(origin));
        float scale=(float)data.getDouble("pixels_per_point");
        // egui's slider rail is the first slider_width logical points, before its label.
        float left=(float)rect.getDouble(0),right=Math.min((float)rect.getDouble(2),left+100f);
        // The pinned egui slider shrinks the rail by its handle radius:
        // height / 2.5, multiplied by the Rect handle aspect ratio 0.75.
        float handle=(float)(rect.getDouble(3)-rect.getDouble(1))/2.5f*0.75f;
        float x=origin[0]+(left+handle+(right-left-2*handle)*fraction)*scale;
        float y=origin[1]+(float)(rect.getDouble(1)+rect.getDouble(3))*scale/2;
        long time=SystemClock.uptimeMillis();
        MotionEvent down=MotionEvent.obtain(time,time,MotionEvent.ACTION_DOWN,x,y,0);sendPointerSync(down);down.recycle();
        // Hold across rendered frames; a future event timestamp alone does not
        // keep the native pointer down while egui evaluates slider interaction.
        SystemClock.sleep(80);
        MotionEvent up=MotionEvent.obtain(time,SystemClock.uptimeMillis(),MotionEvent.ACTION_UP,x,y,0);sendPointerSync(up);up.recycle();SystemClock.sleep(500);
    }
    private void backgroundInput() throws Exception {
        tap("search");tap("query");text("Lluvia",true);
        runOnMainSync(() -> ((android.view.inputmethod.InputMethodManager)activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(activity.getWindow().getDecorView().getWindowToken(),0));
        SystemClock.sleep(400);long before=state().getLong("instance");
        runOnMainSync(() -> activity.moveTaskToBack(true));SystemClock.sleep(600);
        runOnMainSync(() -> getTargetContext().startActivity(new Intent(getTargetContext(),MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)));
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
        resources.put("condition",sample.optJSONObject("audio_metrics").optBoolean("streaming")?"native output from bounded WAV decode buffer; no acoustic capture":"native output from fully prepared PCM; no acoustic capture");
        Files.write(new File(getTargetContext().getFilesDir(),name+".json").toPath(),resources.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
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
        tap("home");tap("local-audio");until(s -> s.optString("playback").equals("Playing"));
        long beforeBackground=state().getLong("instance");
        runOnMainSync(() -> activity.moveTaskToBack(true));SystemClock.sleep(700);
        runOnMainSync(() -> getTargetContext().startActivity(new Intent(getTargetContext(),MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)));
        until(s -> s.optLong("instance")!=beforeBackground && s.optString("playback").equals("Paused"));
        tap("home");tap("local-audio");until(s -> s.optString("playback").equals("Playing"));
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
        runOnMainSync(() -> getTargetContext().startActivity(new Intent(getTargetContext(),MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)));
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
        return new JSONObject(new String(Files.readAllBytes(new File(getTargetContext().getFilesDir(),name).toPath()),java.nio.charset.StandardCharsets.UTF_8));
    }
    private void savedLibrary() throws Exception {
        JSONObject before=state();String original=before.getJSONArray("saved_media").toString();
        boolean existed=savedCount(before,"21923157")>0;
        String legacy=privateDocument("preferences-v1.json").optJSONArray("library").toString();
        tap("search");tap("source-commons");tap("query");text("sine",true);hideKeyboard();
        until(12000,"Commons metadata for favorite",s -> "Ready".equals(s.optJSONObject("online").optString("search_status")));
        tap("save:21923157");tap("save:21923157");
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
        Files.write(new File(getTargetContext().getFilesDir(),"qa-library-checks.json").toPath(),checks.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
        tap("home");
    }
    private void restoredLibrary() throws Exception {
        JSONObject initial=state();
        if(savedCount(initial,"21923157")!=1 || initial.optJSONObject("online").optBoolean("worker_initialized") || !initial.isNull("audio_metrics") || "Playing".equals(initial.optString("playback"))) throw new AssertionError("Library restore started IO/audio or lost qualified favorite");
        tap("home");tap("player");slide("volume",0.08f);
        until(s -> s.optDouble("volume",0)>0.02 && s.optDouble("volume",1)<0.15);
        tap("library");capture("qa-library-restored");
        if(!"demo".equals(state().optJSONObject("online").optString("search_source")) || !"Idle".equals(state().optJSONObject("online").optString("search_status"))) throw new AssertionError("Saved playback must not require search results");
        tap("select:21923157");until(15000,"native output from restored favorite",s -> onlinePlaying(s,"21923157"));
        capture("qa-library-restored-audio");tap("pause");until(s -> "Paused".equals(s.optString("playback")));
    }
    private android.app.UiAutomation automation() {
        return getUiAutomation(android.app.UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES);
    }
    private android.view.accessibility.AccessibilityNodeInfo ownPickerNode(String text) throws Exception {
        long deadline=SystemClock.uptimeMillis()+8000;
        do {
            android.view.accessibility.AccessibilityNodeInfo root=automation().getRootInActiveWindow();
            if(root!=null && "com.android.documentsui".equals(String.valueOf(root.getPackageName()))) {
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
            MotionEvent down=MotionEvent.obtain(time,time,MotionEvent.ACTION_DOWN,x,origin[1]+from*scale,0);sendPointerSync(down);down.recycle();
            for(int step=1;step<=12;step++) {
                SystemClock.sleep(25);MotionEvent move=MotionEvent.obtain(time,SystemClock.uptimeMillis(),MotionEvent.ACTION_MOVE,x,origin[1]+(from+(to-from)*step/12)*scale,0);sendPointerSync(move);move.recycle();
            }
            MotionEvent up=MotionEvent.obtain(time,SystemClock.uptimeMillis(),MotionEvent.ACTION_UP,x,origin[1]+to*scale,0);sendPointerSync(up);up.recycle();SystemClock.sleep(250);
        }
        throw new AssertionError("Document control remained outside the visible content");
    }
    private void documentReturned(long previousInstance) throws Exception {
        foreground();
        until(data -> data.optLong("instance")!=previousInstance);
        // Window focus is delivered before the outgoing picker animation stops
        // intercepting touches on this Samsung/API 28. Do not grant injection
        // permission or send a touch into that foreign transition window.
        SystemClock.sleep(750);foreground();
    }
    private void cancelOwnDocumentPicker() throws Exception {
        long previousInstance=state().getLong("instance");
        // Own folder was verified immediately before entering this method.
        // Back navigates directories before it finishes DocumentsUI on API 28.
        for(int attempt=0;attempt<8;attempt++) {
            android.view.accessibility.AccessibilityNodeInfo root=automation().getRootInActiveWindow();
            String owner=root==null?"":String.valueOf(root.getPackageName());
            if(getTargetContext().getPackageName().equals(owner)) {documentReturned(previousInstance);return;}
            if(!"com.android.documentsui".equals(owner)) throw new AssertionError("Owned document picker lost foreground; no key injected");
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
            if("local-files".equals(item.optString("provider")) && "Original Café estéreo".equals(item.optString("title"))) {
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
        tap("home");tap("player");slide("volume",0.08f);tap("library");
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
            chooseOwnDocument("Original Café estéreo.wav");
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
            capture("qa-document-recreated");tap("home");tap("player");tap("play");
            until(data -> "Playing".equals(data.optString("playback")) && data.optJSONObject("audio_metrics")!=null && data.optJSONObject("audio_metrics").optLong("nonzero_samples")>0);
        }
        tap("pause");until(data -> "Paused".equals(data.optString("playback")));
        long position=state().getLong("position_ms");SystemClock.sleep(400);
        if(state().getLong("position_ms")!=position) throw new AssertionError("Paused document clock advances");
        slide("seek",0.5f);until(data -> data.optLong("position_ms")>55000 && data.optLong("position_ms")<65000);
        if(!restore) {
            tap("library");String saved=state().getJSONArray("saved_media").toString();int files=state().getJSONObject("local_import").getInt("files");
            chooseOwnDocument("Original Café estéreo.wav");
            until(data -> id.equals(data.optString("current")) && "Playing".equals(data.optString("playback")) && data.optJSONObject("audio_metrics")!=null && data.optJSONObject("audio_metrics").optLong("nonzero_samples")>0);
            if(!saved.equals(state().getJSONArray("saved_media").toString()) || state().getJSONObject("local_import").getInt("files")!=files) throw new AssertionError("Repeated document created another identity/favorite");
            tap("settings");tap("light");tap("library");capture("qa-document-library-light");
            tap("settings");tap("dark");tap("library");capture("qa-document-library-dark");
            tap("home");tap("player");tap("pause");
        }
        JSONObject index=privateDocument("local-files-v1.json");
        for(int i=0;i<index.getJSONArray("files").length();i++) {
            String path=index.getJSONArray("files").getJSONObject(i).getString("path");
            if(!new File(path).getCanonicalPath().startsWith(new File(getTargetContext().getFilesDir(),"imported-documents").getCanonicalPath()+"/")) throw new AssertionError("Document locator escaped private storage");
        }
        JSONObject checks=new JSONObject();checks.put("result","pass");checks.put("restored_process",restore);checks.put("id",id);checks.put("acoustic_capture",false);checks.put("state",state());
        Files.write(new File(getTargetContext().getFilesDir(),restore?"qa-document-restore-checks.json":"qa-document-import-checks.json").toPath(),checks.toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
    }
    private InputConnection connection(View view) {
        if(view.onCheckIsTextEditor()) { InputConnection result=view.onCreateInputConnection(new EditorInfo()); if(result!=null) return result; }
        if(view instanceof ViewGroup) { ViewGroup group=(ViewGroup)view; for(int i=0;i<group.getChildCount();i++) { InputConnection result=connection(group.getChildAt(i)); if(result!=null) return result; } }
        return null;
    }
    private void text(String text,boolean burst) throws Exception {
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
        until(s -> text.equals(s.optString("query")));
        SystemClock.sleep(500);
        if(!text.equals(state().getString("query"))) throw new AssertionError("A late frame undid text input");
    }
    @Override public void onStart() {
        Bundle result=new Bundle();
        try {
            Intent intent=new Intent(getTargetContext(),MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            if("document-import".equals(mode)) intent.putExtra("dev.eirom.doremi.qa.documentFolder",documentFolder);
            activity=startActivitySync(intent);
            until(s -> s.optString("page").equals("Home"));
            // Diagnostics are published before GL finishes painting. PixelCopy
            // requires a real first swap before injecting the first touch.
            capture("qa-startup");
            if("document-import".equals(mode) || "document-restore".equals(mode)) {
                documentImport("document-restore".equals(mode));
                result.putString("stream","PASS: explicitly selected private WAV / real bounded native output / controls / identity / original favorites preserved\n");
                finish(Activity.RESULT_OK,result);return;
            }
            if("library-restore".equals(mode)) {
                restoredLibrary();
                result.putString("stream","PASS: saved-library process restoration without automatic IO, qualified-ID playback without search and positive native output\n");
                finish(Activity.RESULT_OK,result);return;
            }
            // Existing development data is retained. Normalize modes through controls.
            tap("search");tap("cancel-search");
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
        } catch(Throwable error) { result.putString("stream","FAIL: "+lastAction+"\n"+android.util.Log.getStackTraceString(error)+"\n");finish(Activity.RESULT_CANCELED,result); }
    }
}
