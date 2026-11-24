package dev.eirom.doremi;

import android.app.*;
import android.content.*;
import android.content.pm.ServiceInfo;
import android.media.*;
import android.media.session.*;
import android.os.*;
import org.json.JSONObject;

/** Platform lifetime and transport only. Rust's single actor owns all music. */
public final class PlaybackService extends Service {
    static { System.loadLibrary("doremi_app"); }
    static native String nativeSnapshot();
    static native int[] nativeArtwork(long version);
    private android.graphics.Bitmap artwork;
    static native boolean nativeCommand(int command,long position,long epoch);
    private static volatile PlaybackService running;
    private static final String CHANNEL="doremi-playback";
    private static final int NOTIFICATION=710;
    private final Handler handler=new Handler(Looper.getMainLooper());
    private MediaSession session;
    private AudioManager audio;
    private AudioFocusRequest focusRequest;
    private PowerManager.WakeLock wake;
    private boolean focusHeld,foreground,closing,transientLoss;
    private long resumeEpoch,resumeIntent,pausedAt,wakeAt;
    static MediaController controller() {return running==null?null:running.session.getController();}
    private JSONObject snapshot=new JSONObject();
    private String published="";

    // Called only following a transport action in a visible Activity. Never
    // started at boot, on process restore, or by an arbitrary background worker.
    static void ensureStarted(Context context) {
        if(running!=null) return;
        try { context.startForegroundService(new Intent(context,PlaybackService.class)); }
        catch(RuntimeException denied) {
            nativeCommand(7,0,0);
            android.util.Log.e("Doremi","Playback foreground service unavailable");
        }
    }
    private final BroadcastReceiver noisy=new BroadcastReceiver() {
        @Override public void onReceive(Context context,Intent intent) {
            if(AudioManager.ACTION_AUDIO_BECOMING_NOISY.equals(intent.getAction())) transport(1,0);
        }
    };
    @Override public void onCreate() {
        super.onCreate();running=this;
        audio=(AudioManager)getSystemService(AUDIO_SERVICE);
        AudioAttributes attributes=new AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build();
        focusRequest=new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
            .setAudioAttributes(attributes).setWillPauseWhenDucked(true)
            .setAcceptsDelayedFocusGain(false).setOnAudioFocusChangeListener(this::focusChanged,handler).build();
        wake=((PowerManager)getSystemService(POWER_SERVICE)).newWakeLock(PowerManager.PARTIAL_WAKE_LOCK,"Doremi:playback");
        wake.setReferenceCounted(false);
        session=new MediaSession(this,"Doremi");
        session.setPlaybackToLocal(attributes);
        session.setSessionActivity(openApp());
        session.setCallback(new MediaSession.Callback() {
            @Override public void onPlay() { transport(0,0); }
            @Override public void onPause() { transport(1,0); }
            @Override public void onSkipToNext() { transport(2,0); }
            @Override public void onSkipToPrevious() { transport(3,0); }
            @Override public void onStop() { transport(4,0); }
            @Override public void onSeekTo(long position) { transport(5,position); }
        },handler);
        session.setActive(true);
        ((NotificationManager)getSystemService(NOTIFICATION_SERVICE)).createNotificationChannel(
            new NotificationChannel(CHANNEL,"Reproducción",NotificationManager.IMPORTANCE_LOW));
        IntentFilter filter=new IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY);
        // Noisy is a protected system broadcast, including on newer Android.
        if(Build.VERSION.SDK_INT>=33) registerReceiver(noisy,filter,Context.RECEIVER_EXPORTED);
        else registerReceiver(noisy,filter);
    }
    @Override public int onStartCommand(Intent intent,int flags,int id) {
        promote(); // Before focus: Android 15+ requires top app or foreground service.
        refresh();
        if(!snapshot.has("epoch")) {finishPlayback();return START_NOT_STICKY;}
        if(intent!=null && intent.hasExtra("command")) {
            long epoch=intent.getLongExtra("epoch",-1);
            if(epoch==snapshot.optLong("epoch",0)) transport(intent.getIntExtra("command",1),0);
        }
        handler.removeCallbacks(poll);handler.post(poll);
        return START_NOT_STICKY;
    }
    private PendingIntent openApp() {
        Intent intent=new Intent(this,MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP);
        if(!snapshot.isNull("profile")) intent.putExtra("dev.eirom.doremi.qa.profile",snapshot.optString("profile")).putExtra("dev.eirom.doremi.qa.internalUi",snapshot.optBoolean("internal_ui"));
        // A QA notification must reconnect to its existing isolated owner.
        return PendingIntent.getActivity(this,0,intent,PendingIntent.FLAG_UPDATE_CURRENT|PendingIntent.FLAG_IMMUTABLE);
    }
    private PendingIntent action(int command) {
        long epoch=snapshot.optLong("epoch",0);
        Intent intent=new Intent(this,PlaybackService.class).setData(android.net.Uri.parse("doremi://transport/"+epoch+"/"+command))
            .putExtra("command",command).putExtra("epoch",epoch);
        return PendingIntent.getForegroundService(this,command,intent,PendingIntent.FLAG_UPDATE_CURRENT|PendingIntent.FLAG_IMMUTABLE);
    }
    private Notification notification() {
        boolean playing=snapshot.optBoolean("active");
        return new Notification.Builder(this,CHANNEL)
            .setSmallIcon(android.R.drawable.ic_media_play).setContentTitle(snapshot.optString("title","Doremi"))
            .setContentText(snapshot.optString("artist","")).setContentIntent(openApp())
            .setLargeIcon(artwork).setOnlyAlertOnce(true).setVisibility(Notification.VISIBILITY_PUBLIC)
            .setOngoing(playing).setCategory(Notification.CATEGORY_TRANSPORT)
            .addAction(new Notification.Action.Builder(android.R.drawable.ic_media_previous,"Anterior",action(3)).build())
            .addAction(new Notification.Action.Builder(playing?android.R.drawable.ic_media_pause:android.R.drawable.ic_media_play,playing?"Pausa":"Reproducir",action(playing?1:0)).build())
            .addAction(new Notification.Action.Builder(android.R.drawable.ic_media_next,"Siguiente",action(2)).build())
            .addAction(new Notification.Action.Builder(android.R.drawable.ic_menu_close_clear_cancel,"Detener",action(4)).build())
            .setStyle(new Notification.MediaStyle().setMediaSession(session.getSessionToken()).setShowActionsInCompactView(0,1,2)).build();
    }
    private void promote() {
        if(foreground || closing) return;
        if(Build.VERSION.SDK_INT>=29) startForeground(NOTIFICATION,notification(),ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK);
        else startForeground(NOTIFICATION,notification());
        foreground=true;
    }
    private void refresh() {
        try {snapshot=new JSONObject(nativeSnapshot());} catch(Exception ignored) {snapshot=new JSONObject();}
    }
    private void transport(int command,long position) {
        if(closing) return;
        refresh();resumeEpoch=0;transientLoss=false;
        if(command==4) {finishPlayback();return;}
        if(command==0 || command==2 || command==3) {
            promote();
            if(!requestFocus()) return;
        }
        if(!nativeCommand(command,position,snapshot.optLong("epoch",0))) {
            android.util.Log.e("Doremi","Playback mailbox full");
        }
        if(command==1) {releaseFocus();releaseWake();}
        handler.removeCallbacks(poll);handler.postDelayed(poll,100);
    }
    private boolean requestFocus() {
        if(focusHeld) return true;
        focusHeld=audio.requestAudioFocus(focusRequest)==AudioManager.AUDIOFOCUS_REQUEST_GRANTED;
        nativeCommand(focusHeld?6:7,0,0);
        return focusHeld;
    }
    private void focusChanged(int change) {
        if(closing) return;
        refresh();
        if(change==AudioManager.AUDIOFOCUS_GAIN) {
            focusHeld=true;transientLoss=false;
            nativeCommand(6,0,0);
            if(resumeEpoch!=0 && resumeEpoch==snapshot.optLong("epoch") && resumeIntent==snapshot.optLong("intent")) nativeCommand(0,0,resumeEpoch);
            resumeEpoch=0;
        } else {
            boolean temporary=change==AudioManager.AUDIOFOCUS_LOSS_TRANSIENT || change==AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK;
            resumeEpoch=temporary && snapshot.optBoolean("active")?snapshot.optLong("epoch"):0;
            resumeIntent=snapshot.optLong("intent");
            transientLoss=temporary;focusHeld=false;
            nativeCommand(7,0,0);releaseWake();
            if(!temporary) releaseFocus();
        }
    }
    private void releaseFocus() {
        audio.abandonAudioFocusRequest(focusRequest);focusHeld=false;
    }
    private void releaseWake() {if(wake.isHeld()) wake.release();wakeAt=0;}
    private final Runnable poll=new Runnable() {
        @Override public void run() {
            if(closing) return;
            refresh();
            boolean active=snapshot.optBoolean("active");
            long now=SystemClock.elapsedRealtime();
            if(active && !transientLoss) {
                pausedAt=0;promote();requestFocus();
                // Bounded lease, only while active; renewed before expiry. Full
                // download and software decoding need CPU with the screen off.
                if(wakeAt==0 || now-wakeAt>300000) {wake.acquire(600000);wakeAt=now;}
            } else {
                releaseWake();
                if(!transientLoss) releaseFocus();
                if(pausedAt==0) pausedAt=now;
                if(foreground && now-pausedAt>30000) {stopForeground(STOP_FOREGROUND_DETACH);foreground=false;}
                if(now-pausedAt>300000) {finishPlayback();return;}
            }
            String state=snapshot.optString("state");
            int status="Playing".equals(state)?PlaybackState.STATE_PLAYING:"Loading".equals(state) && active?PlaybackState.STATE_BUFFERING:
                "Failed".equals(state)?PlaybackState.STATE_ERROR:"Stopped".equals(state)?PlaybackState.STATE_STOPPED:PlaybackState.STATE_PAUSED;
            long actions=PlaybackState.ACTION_PLAY|PlaybackState.ACTION_PAUSE|PlaybackState.ACTION_PLAY_PAUSE|PlaybackState.ACTION_STOP|
                PlaybackState.ACTION_SKIP_TO_NEXT|PlaybackState.ACTION_SKIP_TO_PREVIOUS|PlaybackState.ACTION_SEEK_TO;
            session.setPlaybackState(new PlaybackState.Builder().setActions(actions)
                .setState(status,snapshot.optLong("position"),status==PlaybackState.STATE_PLAYING?1f:0f,SystemClock.elapsedRealtime()).build());
            String key=snapshot.optLong("epoch")+":"+state+":"+snapshot.optString("artist")+":"+snapshot.optString("album")+":"+snapshot.optString("title")+":"+snapshot.optLong("duration")+":"+snapshot.optLong("artwork_version");
            if(!key.equals(published)) {
                published=key;
                MediaMetadata.Builder metadata=new MediaMetadata.Builder();
                if(!snapshot.isNull("title")) metadata.putString(MediaMetadata.METADATA_KEY_TITLE,snapshot.optString("title"));
                if(!snapshot.isNull("artist")) metadata.putString(MediaMetadata.METADATA_KEY_ARTIST,snapshot.optString("artist"));
                if(!snapshot.isNull("album")) metadata.putString(MediaMetadata.METADATA_KEY_ALBUM,snapshot.optString("album"));
                if(!snapshot.isNull("duration")) metadata.putLong(MediaMetadata.METADATA_KEY_DURATION,snapshot.optLong("duration"));
                int[] pixels=snapshot.isNull("artwork_version")?null:nativeArtwork(snapshot.optLong("artwork_version"));
                artwork=pixels==null?null:android.graphics.Bitmap.createBitmap(pixels,2,pixels[0],pixels[0],pixels[1],android.graphics.Bitmap.Config.ARGB_8888);
                if(artwork!=null) metadata.putBitmap(MediaMetadata.METADATA_KEY_ALBUM_ART,artwork);
                session.setMetadata(metadata.build());
                ((NotificationManager)getSystemService(NOTIFICATION_SERVICE)).notify(NOTIFICATION,notification());
            }
            if(snapshot.optBoolean("stopped")) {finishPlayback();return;}
            handler.postDelayed(this,active?500:1000);
        }
    };
    private void finishPlayback() {
        if(closing) return;
        closing=true;nativeCommand(4,0,0);handler.removeCallbacksAndMessages(null);resumeEpoch=0;
        releaseFocus();releaseWake();session.setActive(false);
        stopForeground(STOP_FOREGROUND_REMOVE);((NotificationManager)getSystemService(NOTIFICATION_SERVICE)).cancel(NOTIFICATION);
        stopSelf();
    }
    @Override public void onDestroy() {
        finishPlayback();
        session.setCallback(null);session.release();unregisterReceiver(noisy);
        running=null;super.onDestroy();
    }
    @Override public IBinder onBind(Intent intent) {return null;}
}
