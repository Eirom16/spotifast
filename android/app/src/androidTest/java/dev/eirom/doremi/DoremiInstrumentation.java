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
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }
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
            final Bitmap[] image={null};final int[] status={-1};CountDownLatch copied=new CountDownLatch(1);
            runOnMainSync(() -> {
                SurfaceView view=surface(activity.getWindow().getDecorView());
                if(view==null || view.getWidth()==0 || view.getHeight()==0) {copied.countDown();return;}
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
                Files.write(new File(getTargetContext().getFilesDir(),name+".json").toPath(),state().toString().getBytes(java.nio.charset.StandardCharsets.UTF_8));
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
        runOnMainSync(() -> activity.findViewById(android.R.id.content).getLocationOnScreen(origin));
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
        final int[] origin=new int[2];runOnMainSync(() -> activity.findViewById(android.R.id.content).getLocationOnScreen(origin));
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
        resources.put("condition","native output from fully prepared PCM; no acoustic capture");
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
            activity=startActivitySync(intent);
            until(s -> s.optString("page").equals("Home"));
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
            online();
            result.putString("stream","PASS: Unicode/burst IME, Activity recreation with duplicate queue, landscape/portrait, shared rain -> dawn -> dawn -> cafe trace; IME cancel after background; native PCM play/pause/seek/volume/next/previous/EOF/cancel/focus/recreation/background; real Commons search/load/cancel/PCM/next/previous/paused recreation/background\n");
            finish(Activity.RESULT_OK,result);
        } catch(Throwable error) { result.putString("stream","FAIL: "+lastAction+"\n"+android.util.Log.getStackTraceString(error)+"\n");finish(Activity.RESULT_CANCELED,result); }
    }
}
