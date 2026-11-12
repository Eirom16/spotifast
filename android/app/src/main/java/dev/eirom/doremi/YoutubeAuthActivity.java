package dev.eirom.doremi;

import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.view.WindowManager;
import android.webkit.CookieManager;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;

/** Disposable authentication host, in its own process. Never a music renderer. */
public final class YoutubeAuthActivity extends Activity {
    static final String GENERATION="dev.eirom.doremi.auth.generation";
    static final String COOKIE="dev.eirom.doremi.auth.cookie";
    static final String STATUS="dev.eirom.doremi.auth.status";
    private static final String MUSIC="https://music.youtube.com/";
    private static final String LOGIN="https://accounts.google.com/ServiceLogin?continue=https%3A%2F%2Fmusic.youtube.com";
    private final Handler handler=new Handler(Looper.getMainLooper());
    private WebView browser;
    private boolean completed=false;
    private long generation;
    private boolean readingContext=false;
    private int contextAttempts=0;
    private long readGeneration=0,readDeadline=0;
    private final Runnable timeout=() -> complete("",2);
    private final Runnable poll=new Runnable() {
        @Override public void run() {
            if(completed || browser==null) return;
            String cookie=CookieManager.getInstance().getCookie(MUSIC);
            if(cookie!=null && cookie.length()>65536) {complete("",5);return;}
            if(readingContext && android.os.SystemClock.uptimeMillis()>=readDeadline) readingContext=false;
            if(sessionReady(cookie) && musicPage(browser.getUrl()) && !readingContext) {
                if(++contextAttempts>20) {complete("",4);return;}
                if(readGeneration==Long.MAX_VALUE) {complete("",4);return;}
                long expected=++readGeneration;
                readingContext=true;
                readDeadline=android.os.SystemClock.uptimeMillis()+2000;
                // Read service-owned configuration only on Music. No bridge is
                // exposed to Google pages, no page APIs/permissions overridden.
                browser.evaluateJavascript("(function(){if(location.origin!=='https://music.youtube.com')return null;var c=(window.yt&&window.yt.config_)||(window.ytcfg&&window.ytcfg.data_)||{};return JSON.stringify({visitor_data:c.VISITOR_DATA||'',data_sync_id:c.DATASYNC_ID||'',auth_user:String(c.SESSION_INDEX||0)});})()",value -> {
                    if(expected!=readGeneration) return;
                    readingContext=false;
                    if(completed || browser==null || !musicPage(browser.getUrl())) return;
                    if(value==null || value.length()>20000) return;
                    try {
                        Object decoded=new org.json.JSONTokener(value).nextValue();
                        if(!(decoded instanceof String)) return;
                        String current=CookieManager.getInstance().getCookie(MUSIC);
                        String document=sessionDocument(current,(String)decoded);
                        if(document!=null) complete(document,0);
                    } catch(org.json.JSONException invalid) { /* Next bounded poll. */ }
                });
            }
            handler.postDelayed(this,500);
        }
    };
    static boolean sessionReady(String cookie) {
        return cookie!=null && cookie.length()<=65536 && !cookie.contains("\r") && !cookie.contains("\n") &&
            (has(cookie,"SAPISID") || has(cookie,"__Secure-3PAPISID"));
    }
    static String sessionDocument(String cookie,String context) {
        if(!sessionReady(cookie) || context==null || context.length()>20000) return null;
        try {
            org.json.JSONObject values=new org.json.JSONObject(context);
            Object visitorValue=values.opt("visitor_data");
            if(!(visitorValue instanceof String)) return null;
            String visitor=(String)visitorValue;
            String user=values.optString("auth_user","0");
            if(visitor.isEmpty() || visitor.length()>8192 || !user.matches("[0-9]{1,2}")) return null;
            String document=new org.json.JSONObject().put("schema",1).put("cookie",cookie)
                .put("visitor_data",visitor).put("data_sync_id",values.optString("data_sync_id",""))
                .put("auth_user",user).toString();
            return document.length()<=65536?document:null;
        } catch(org.json.JSONException invalid) {return null;}
    }
    private static boolean musicPage(String value) {
        return value!=null && allowed(value) && "music.youtube.com".equals(Uri.parse(value).getHost());
    }
    static Intent authIntent(String origin,String target) {
        if(origin==null || !allowed(origin) || target==null || target.length()>16384 || !"intent".equals(Uri.parse(target).getScheme())) return null;
        try {
            Intent intent=Intent.parseUri(target,Intent.URI_INTENT_SCHEME);
            Uri data=intent.getData();
            if(data==null || data.getScheme()==null) return null;
            boolean knownPackage=java.util.Arrays.asList("com.google.android.gms","com.google.android.googlequicksearchbox","com.android.chrome").contains(intent.getPackage());
            boolean web="https".equals(data.getScheme()) && allowed(data.toString()) && !denied(data.toString()) && (intent.getPackage()==null || knownPackage);
            boolean googleApp=knownPackage &&
                java.util.Arrays.asList("googleapp","googlechrome","googleauth","android-app").contains(data.getScheme());
            if(!web && !googleApp) return null;
            intent.setAction(Intent.ACTION_VIEW);intent.addCategory(Intent.CATEGORY_BROWSABLE);
            intent.setComponent(null);intent.setSelector(null);
            intent.setFlags(intent.getFlags() & (Intent.FLAG_ACTIVITY_NEW_TASK | Intent.FLAG_ACTIVITY_CLEAR_TOP | Intent.FLAG_ACTIVITY_SINGLE_TOP));
            return intent;
        } catch(java.net.URISyntaxException invalid) {return null;}
    }
    private boolean handoff(WebView view,String target) {
        Intent intent=authIntent(view.getUrl(),target);
        if(intent==null) {complete("",4);return true;}
        String fallback=intent.getStringExtra("browser_fallback_url");intent.removeExtra("browser_fallback_url");
        try {startActivity(intent);}
        catch(ActivityNotFoundException | SecurityException unavailable) {
            if(fallback!=null && allowed(fallback) && !denied(fallback)) view.loadUrl(fallback);
            else complete("",4);
        }
        return true;
    }
    private static boolean has(String cookie,String name) {
        for(String pair:cookie.split(";")) if(pair.trim().startsWith(name+"=") && pair.trim().length()>name.length()+1) return true;
        return false;
    }
    static boolean allowed(String value) {
        Uri uri=Uri.parse(value);String host=uri.getHost();
        if(!"https".equals(uri.getScheme()) || uri.getPort()!=-1 || uri.getUserInfo()!=null || host==null) return false;
        return java.util.Arrays.asList("accounts.google.com","consent.google.com","myaccount.google.com",
            "music.youtube.com","www.youtube.com","youtube.com","accounts.youtube.com","consent.youtube.com").contains(host);
    }
    static boolean denied(String value) {
        Uri uri=Uri.parse(value);
        return "accounts.google.com".equals(uri.getHost()) &&
            ((uri.getPath()!=null && uri.getPath().contains("/signin/rejected")) ||
             "disallowed_useragent".equals(uri.getQueryParameter("error")) || "access_denied".equals(uri.getQueryParameter("error")));
    }
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        getWindow().addFlags(WindowManager.LayoutParams.FLAG_SECURE);
        generation=getIntent().getLongExtra(GENERATION,0);
        if(generation<=0 || state!=null) {complete("",1);return;}
        handler.postDelayed(timeout,600000);
        try {
            // No other Doremi process creates a WebView. Never open an existing
            // browser profile; retire this process completely after the result.
            if(Build.VERSION.SDK_INT>=28) WebView.setDataDirectorySuffix("youtube-auth");
            WebView.setWebContentsDebuggingEnabled(false);
            browser=new WebView(this);
            WebSettings settings=browser.getSettings();
            settings.setJavaScriptEnabled(true);
            settings.setDomStorageEnabled(true);
            settings.setAllowFileAccess(false);
            settings.setAllowContentAccess(false);
            settings.setMediaPlaybackRequiresUserGesture(true);
            settings.setMixedContentMode(WebSettings.MIXED_CONTENT_NEVER_ALLOW);
            settings.setCacheMode(WebSettings.LOAD_NO_CACHE);
            settings.setSaveFormData(false);
            browser.setDownloadListener((url,agent,disposition,type,length) -> complete("",3));
            browser.setWebViewClient(new WebViewClient() {
                @Override public void onPageStarted(WebView view,String url,android.graphics.Bitmap icon) {
                    if(readGeneration==Long.MAX_VALUE) {complete("",4);return;}
                    readGeneration++;readingContext=false;contextAttempts=0;
                }
                @Override public boolean shouldOverrideUrlLoading(WebView view,WebResourceRequest request) {
                    if("intent".equals(request.getUrl().getScheme())) return handoff(view,request.getUrl().toString());
                    if(!request.isForMainFrame()) return false;
                    String url=request.getUrl().toString();
                    if(denied(url)) {complete("",3);return true;}
                    return !allowed(url);
                }
                @Override public void onPageFinished(WebView view,String url) {
                    if(denied(url)) complete("",3);
                }
                // Default TLS failure handling cancels; never override trust.
            });
            setContentView(browser);
            CookieManager cookies=CookieManager.getInstance();
            cookies.setAcceptCookie(true);
            // The temporary sign-in crosses Google and YouTube origins. Use
            // WebView's normal per-view cookie policy; destroy and wipe this
            // isolated browser after the result. Never override storage APIs.
            cookies.setAcceptThirdPartyCookies(browser,true);
            cookies.removeAllCookies(removed -> {
                // Platform flow observed in Meld: Google supplies the complete
                // return journey to Music; native intent handoffs stay usable.
                if(!completed && browser!=null) {browser.loadUrl(LOGIN);handler.post(poll);}
            });
        } catch(RuntimeException unavailable) {complete("",4);}
    }
    private void complete(String cookie,int status) {
        if(completed) return;
        completed=true;handler.removeCallbacksAndMessages(null);
        // Only an explicit, non-exported Activity result carries the secret.
        // It is never saved in state, logged, placed on the clipboard or URL.
        Intent result=new Intent().putExtra(GENERATION,generation).putExtra(STATUS,status);
        if(status==0) result.putExtra(COOKIE,cookie);
        setResult(status==0?RESULT_OK:RESULT_CANCELED,result);
        if(browser!=null) {
            browser.stopLoading();browser.clearHistory();browser.clearCache(true);
            android.webkit.WebStorage.getInstance().deleteAllData();
            android.view.ViewParent parent=browser.getParent();
            if(parent instanceof android.view.ViewGroup) ((android.view.ViewGroup)parent).removeView(browser);
            browser.destroy();browser=null;
            try {CookieManager.getInstance().removeAllCookies(removed -> finish());}
            catch(RuntimeException unavailable) {finish();}
            handler.postDelayed(this::finish,1500);
        } else finish();
    }
    @Override public void onBackPressed() {complete("",1);}
    @Override protected void onDestroy() {
        handler.removeCallbacksAndMessages(null);
        if(browser!=null) {browser.stopLoading();browser.destroy();browser=null;}
        super.onDestroy();
        // The browser engine and renderer must not remain in the music process
        // or consume resources after authentication ends.
        Runnable retire=() -> android.os.Process.killProcess(android.os.Process.myPid());
        try {
            android.webkit.WebStorage.getInstance().deleteAllData();
            CookieManager.getInstance().removeAllCookies(removed -> retire.run());
            handler.postDelayed(retire,1500);
        } catch(RuntimeException unavailable) {retire.run();}
    }
}
