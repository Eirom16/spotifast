package dev.eirom.doremi;

import android.app.Activity;
import android.app.Instrumentation;
import android.content.Intent;
import android.os.Bundle;
import android.os.SystemClock;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputConnection;
import org.json.JSONArray;
import org.json.JSONObject;
import java.io.File;
import java.nio.file.Files;

/** Exercises Android input and Activity recreation without a custom IME or global settings. */
public final class DoremiInstrumentation extends Instrumentation {
    private Activity activity;
    @Override public void onCreate(Bundle arguments) { super.onCreate(arguments); start(); }
    private JSONObject state() throws Exception {
        return new JSONObject(new String(Files.readAllBytes(new File(getTargetContext().getFilesDir(), "qa-state.json").toPath()), java.nio.charset.StandardCharsets.UTF_8));
    }
    private void tap(String name) throws Exception {
        JSONObject data = state();
        JSONArray controls = data.getJSONArray("controls");
        JSONArray rect = null;
        for (int i=0; i<controls.length(); i++) {
            JSONObject control = controls.getJSONObject(i);
            if (name.equals(control.getString("name"))) { rect = control.getJSONArray("rect"); break; }
        }
        if (rect == null) throw new AssertionError("Missing control: " + name);
        final int[] origin = new int[2];
        runOnMainSync(() -> activity.findViewById(android.R.id.content).getLocationOnScreen(origin));
        float scale = (float)data.getDouble("pixels_per_point");
        float x = origin[0] + (float)(rect.getDouble(0)+rect.getDouble(2))*scale/2;
        float y = origin[1] + (float)(rect.getDouble(1)+rect.getDouble(3))*scale/2;
        long time=SystemClock.uptimeMillis();
        MotionEvent down=MotionEvent.obtain(time,time,MotionEvent.ACTION_DOWN,x,y,0);
        sendPointerSync(down); down.recycle();
        MotionEvent up=MotionEvent.obtain(time,time+80,MotionEvent.ACTION_UP,x,y,0);
        sendPointerSync(up); up.recycle();
        SystemClock.sleep(700);
    }
    private InputConnection connection(View view) {
        if (view.onCheckIsTextEditor()) {
            InputConnection result=view.onCreateInputConnection(new EditorInfo());
            if(result!=null) return result;
        }
        if(view instanceof ViewGroup) {
            ViewGroup group=(ViewGroup)view;
            for(int i=0;i<group.getChildCount();i++) {
                InputConnection result=connection(group.getChildAt(i)); if(result!=null) return result;
            }
        }
        return null;
    }
    @Override public void onStart() {
        Bundle result=new Bundle();
        try {
            Intent intent=new Intent(getTargetContext(),MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
            activity=startActivitySync(intent);
            SystemClock.sleep(1800);
            tap("search"); tap("query");
            final boolean[] committed={false};
            runOnMainSync(() -> {
                InputConnection input=connection(activity.getWindow().getDecorView());
                if(input!=null) { input.beginBatchEdit(); input.deleteSurroundingText(512,512); committed[0]=input.commitText("Café",1); input.endBatchEdit(); }
            });
            if(!committed[0]) throw new AssertionError("GameActivity InputConnection unavailable");
            SystemClock.sleep(1000);
            if(!"Café".equals(state().getString("query"))) throw new AssertionError("Unicode text did not reach Rust");
            // Hide only this app's keyboard before exercising recreation.
            runOnMainSync(() -> ((android.view.inputmethod.InputMethodManager)activity.getSystemService(android.content.Context.INPUT_METHOD_SERVICE)).hideSoftInputFromWindow(activity.getWindow().getDecorView().getWindowToken(),0));
            long revision=state().getLong("revision");
            long instance=state().getLong("instance");
            runOnMainSync(() -> activity.recreate());
            long deadline=SystemClock.uptimeMillis()+8000;
            while(SystemClock.uptimeMillis()<deadline && state().getLong("instance")==instance) SystemClock.sleep(200);
            JSONObject after=state();
            if(after.getLong("instance")<=instance) throw new AssertionError("No new Rust UI instance after Activity recreation");
            if(!"Café".equals(after.getString("query")) || after.getLong("revision")<revision) throw new AssertionError("Activity recreation lost state");
            result.putString("stream","PASS: Unicode Android InputConnection and Activity recreation\n");
            finish(Activity.RESULT_OK,result);
        } catch(Throwable error) {
            result.putString("stream","FAIL: "+error+"\n"); finish(Activity.RESULT_CANCELED,result);
        }
    }
}
