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
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }
    private JSONObject state() throws Exception {
        return new JSONObject(new String(Files.readAllBytes(new File(getTargetContext().getFilesDir(),"qa-state.json").toPath()),java.nio.charset.StandardCharsets.UTF_8));
    }
    private JSONObject until(Predicate<JSONObject> predicate) throws Exception {
        long deadline=SystemClock.uptimeMillis()+8000;
        while(SystemClock.uptimeMillis()<deadline) {
            JSONObject data=state();
            if(data.optInt("pid")==android.os.Process.myPid() && predicate.test(data)) return data;
            SystemClock.sleep(100);
        }
        throw new AssertionError("Expected fresh Rust state was not observed");
    }
    private SurfaceView surface(View view) {
        if(view instanceof SurfaceView) return (SurfaceView)view;
        if(view instanceof ViewGroup) { ViewGroup group=(ViewGroup)view; for(int i=0;i<group.getChildCount();i++) { SurfaceView found=surface(group.getChildAt(i));if(found!=null) return found; } }
        return null;
    }
    private void capture(String name) throws Exception {
        final Bitmap[] image={null}; final int[] status={-1}; CountDownLatch copied=new CountDownLatch(1);
        runOnMainSync(() -> {
            SurfaceView view=surface(activity.getWindow().getDecorView());
            if(view==null || view.getWidth()==0 || view.getHeight()==0) { copied.countDown(); return; }
            image[0]=Bitmap.createBitmap(view.getWidth(),view.getHeight(),Bitmap.Config.ARGB_8888);
            // Copy only Doremi's own EGL surface, never the launcher or another app.
            PixelCopy.request(view,image[0],value -> { status[0]=value;copied.countDown(); },new Handler(Looper.getMainLooper()));
        });
        if(!copied.await(3,TimeUnit.SECONDS) || status[0]!=PixelCopy.SUCCESS) throw new AssertionError("Doremi surface capture failed: "+status[0]);
        try(FileOutputStream file=new FileOutputStream(new File(getTargetContext().getFilesDir(),name+".png"))) { image[0].compress(Bitmap.CompressFormat.PNG,100,file); }
        image[0].recycle();
    }
    private void tap(String name) throws Exception {
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
        MotionEvent up=MotionEvent.obtain(time,time+80,MotionEvent.ACTION_UP,x,y,0); sendPointerSync(up); up.recycle();
        SystemClock.sleep(500);
    }
    private void slide(String name,float fraction) throws Exception {
        JSONObject data=state(); JSONArray controls=data.getJSONArray("controls");JSONArray rect=null;
        for(int i=0;i<controls.length();i++) {JSONObject c=controls.getJSONObject(i);if(name.equals(c.getString("name"))) rect=c.getJSONArray("rect");}
        if(rect==null) throw new AssertionError("Missing slider: "+name);
        final int[] origin=new int[2];runOnMainSync(() -> activity.findViewById(android.R.id.content).getLocationOnScreen(origin));
        float scale=(float)data.getDouble("pixels_per_point");
        // egui's slider rail is the first slider_width logical points, before its label.
        float left=(float)rect.getDouble(0),right=Math.min((float)rect.getDouble(2),left+100f);
        float x=origin[0]+(left+8f+(right-left-16f)*fraction)*scale;
        float y=origin[1]+(float)(rect.getDouble(1)+rect.getDouble(3))*scale/2;
        long time=SystemClock.uptimeMillis();
        MotionEvent down=MotionEvent.obtain(time,time,MotionEvent.ACTION_DOWN,x,y,0);sendPointerSync(down);down.recycle();
        MotionEvent up=MotionEvent.obtain(time,time+80,MotionEvent.ACTION_UP,x,y,0);sendPointerSync(up);up.recycle();SystemClock.sleep(500);
    }
    private void audio() throws Exception {
        tap("home");tap("local-audio");
        until(s -> s.optBoolean("audio") && s.optString("playback").equals("Playing") && s.optLong("position_ms")>300 && s.optJSONObject("audio_metrics")!=null && s.optJSONObject("audio_metrics").optLong("frames")>0);
        capture("qa-audio-playing");
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
        until(s -> s.optString("current").equals("tone-chord") && s.optLong("position_ms")>100);
        capture("qa-audio-after-end");
        tap("stop");until(s -> s.optString("playback").equals("Paused") && s.isNull("audio_metrics"));
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
            audio();
            result.putString("stream","PASS: Unicode/burst IME, Activity recreation with duplicate queue, landscape/portrait, shared rain -> dawn -> dawn -> cafe trace; native PCM play/pause/seek/volume/next/previous/EOF/cancel/focus/recreation/background\n");
            finish(Activity.RESULT_OK,result);
        } catch(Throwable error) { result.putString("stream","FAIL: "+error+"\n");finish(Activity.RESULT_CANCELED,result); }
    }
}
