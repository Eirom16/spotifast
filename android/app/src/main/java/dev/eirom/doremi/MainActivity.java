package dev.eirom.doremi;

import android.app.Activity;
import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.content.IntentFilter;
import android.media.AudioAttributes;
import android.media.AudioFocusRequest;
import android.media.AudioManager;
import android.os.Build;
import android.os.Bundle;
import android.opengl.GLSurfaceView;
import android.view.MotionEvent;
import android.view.View;
import android.view.inputmethod.InputMethodManager;
import android.widget.EditText;
import android.widget.FrameLayout;
import android.widget.Toast;
import android.text.Editable;
import android.text.TextWatcher;
import org.json.JSONObject;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import javax.microedition.khronos.egl.EGLConfig;
import javax.microedition.khronos.opengles.GL10;

/** Android EGL, touch, IME and lifecycle only. All music rules remain in Rust. */
public final class MainActivity extends Activity {
    static { System.loadLibrary("doremi_app"); }
    private static native boolean nativeInit(String directory,Context application);
    private static native String nativeDraw(int width,int height,float scale);
    private static native void nativeTouch(int action,float x,float y);
    private static native void nativeText(String text,long generation);
    private static native void nativeDestroy();
    private static native void nativeFocus(boolean granted);
    private AudioManager audioManager;
    private AudioFocusRequest focusRequest;
    private boolean focusHeld=false;
    private final BroadcastReceiver noisy=new BroadcastReceiver() {
        @Override public void onReceive(Context context,Intent intent) {
            if(AudioManager.ACTION_AUDIO_BECOMING_NOISY.equals(intent.getAction())) loseFocus();
        }
    };
    private void sendFocus(boolean granted) {
        if(surface!=null) { surface.queueEvent(() -> nativeFocus(granted));surface.requestRender(); }
    }
    private void loseFocus() {
        if(focusHeld) audioManager.abandonAudioFocusRequest(focusRequest);
        focusHeld=false; sendFocus(false);
    }
    private void updateFocus(boolean needed,boolean loading) {
        if(loading) return; // Keep an existing grant across a core-owned track transition.
        if(needed && !focusHeld) {
            focusHeld=audioManager.requestAudioFocus(focusRequest)==AudioManager.AUDIOFOCUS_REQUEST_GRANTED;
            sendFocus(focusHeld);
        } else if(!needed && focusHeld) {
            audioManager.abandonAudioFocusRequest(focusRequest);focusHeld=false;
            // Pausing is already decided in the core. Clear only the platform gate.
            sendFocus(false);
        }
    }
    private GLSurfaceView surface;
    private EditText input;
    private final HostRenderer renderer=new HostRenderer();
    private boolean updatingInput=false;
    private volatile long inputGeneration=0;
    private boolean imeRequested=false;
    private volatile boolean resumed=false;
    private float scale;
    private final Runnable repaint=() -> { if(resumed && surface!=null) surface.requestRender(); };

    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        audioManager=(AudioManager)getSystemService(AUDIO_SERVICE);
        focusRequest=new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
            .setWillPauseWhenDucked(true).setAcceptsDelayedFocusGain(false)
            .setOnAudioFocusChangeListener(change -> { if(change!=AudioManager.AUDIOFOCUS_GAIN) loseFocus(); }).build();
        setVolumeControlStream(AudioManager.STREAM_MUSIC);
        scale=getResources().getDisplayMetrics().density;
        FrameLayout root=new FrameLayout(this);
        root.setOnApplyWindowInsetsListener((view,insets) -> {
            view.setPadding(insets.getSystemWindowInsetLeft(),insets.getSystemWindowInsetTop(),insets.getSystemWindowInsetRight(),insets.getSystemWindowInsetBottom());
            return insets;
        });
        surface=new GLSurfaceView(this);
        surface.setEGLContextClientVersion(3);
        surface.setRenderer(renderer);
        surface.setRenderMode(GLSurfaceView.RENDERMODE_WHEN_DIRTY);
        surface.setOnTouchListener((view,event) -> {
            int action=event.getActionMasked();
            if(action>3) return true; // One primary pointer, no synthetic extra taps.
            float x=event.getX()/scale,y=event.getY()/scale;
            if(action==MotionEvent.ACTION_DOWN) imeRequested=false;
            surface.queueEvent(() -> nativeTouch(action,x,y));
            surface.requestRender(); return true;
        });
        root.addView(surface,new FrameLayout.LayoutParams(-1,-1));
        // Android owns composing text, cursor/selection and Unicode input.
        // egui draws the visible field. This private 1px editor is only its IME bridge.
        input=new EditText(this);
        input.setSingleLine(true);
        input.setInputType(android.text.InputType.TYPE_CLASS_TEXT);
        input.setFilters(new android.text.InputFilter[]{new android.text.InputFilter.LengthFilter(128)});
        input.setAlpha(0f);
        input.setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_YES);
        input.setContentDescription("Buscar en Doremi");
        input.addTextChangedListener(new TextWatcher() {
            public void beforeTextChanged(CharSequence s,int start,int count,int after) {}
            public void onTextChanged(CharSequence s,int start,int before,int count) {}
            public void afterTextChanged(Editable text) {
                if(!updatingInput) { String value=text.toString(); long generation=++inputGeneration; surface.queueEvent(() -> nativeText(value,generation)); surface.requestRender(); }
            }
        });
        root.addView(input,new FrameLayout.LayoutParams(1,1));
        surface.setFocusableInTouchMode(true); surface.requestFocus();
        setContentView(root); root.requestApplyInsets();
    }
    @Override protected void onResume() {
        super.onResume(); resumed=true;
        IntentFilter filter=new IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY);
        if(Build.VERSION.SDK_INT>=33) registerReceiver(noisy,filter,Context.RECEIVER_NOT_EXPORTED);
        else registerReceiver(noisy,filter);
        if(surface!=null) { surface.onResume(); surface.requestRender(); }
    }
    @Override protected void onPause() {
        resumed=false; unregisterReceiver(noisy); loseFocus(); surface.removeCallbacks(repaint);
        CountDownLatch stopped=new CountDownLatch(1);
        surface.queueEvent(() -> { try { nativeDestroy(); renderer.ready=false; } finally { stopped.countDown(); } });
        try { if(!stopped.await(2,TimeUnit.SECONDS)) android.util.Log.e("Doremi","GL shutdown timeout"); }
        catch(InterruptedException error) { Thread.currentThread().interrupt(); }
        surface.onPause(); super.onPause();
    }
    private final class HostRenderer implements GLSurfaceView.Renderer {
        private boolean ready=false;
        private int width,height;
        public void onSurfaceCreated(GL10 gl,EGLConfig config) { nativeDestroy(); ready=false; }
        public void onSurfaceChanged(GL10 gl,int w,int h) { width=w; height=h; }
        public void onDrawFrame(GL10 gl) {
            if(!ready) {
                ready=nativeInit(getFilesDir().getAbsolutePath(),getApplicationContext());
                if(!ready) { runOnUiThread(() -> Toast.makeText(MainActivity.this,"No se pudo iniciar OpenGL de Doremi",Toast.LENGTH_LONG).show()); return; }
            }
            String output=nativeDraw(width,height,scale);
            try {
                JSONObject data=new JSONObject(output);
                boolean ime=data.optBoolean("ime");
                boolean needsFocus=data.optBoolean("needs_focus");
                boolean loadingAudio=data.optBoolean("loading_audio");
                String query=data.optString("query");
                long delay=data.optLong("next_ms",10000);
                long acknowledgedGeneration=data.optLong("input_generation",0);
                runOnUiThread(() -> {
                    if(!resumed) return;
                    updateFocus(needsFocus,loadingAudio);
                    updatingInput=true;
                    if(acknowledgedGeneration==inputGeneration && !input.getText().toString().equals(query)) { input.setText(query); input.setSelection(input.length()); }
                    updatingInput=false;
                    InputMethodManager manager=(InputMethodManager)getSystemService(INPUT_METHOD_SERVICE);
                    if(ime && !imeRequested) { imeRequested=true; input.requestFocus(); manager.showSoftInput(input,InputMethodManager.SHOW_IMPLICIT); }
                    if(!ime && imeRequested) { imeRequested=false; manager.hideSoftInputFromWindow(input.getWindowToken(),0); surface.requestFocus(); }
                    surface.removeCallbacks(repaint);
                    if(delay<10000) surface.postDelayed(repaint,Math.max(16,delay));
                });
            } catch(Exception error) { android.util.Log.e("Doremi","Invalid renderer response",error); }
        }
    }
}
