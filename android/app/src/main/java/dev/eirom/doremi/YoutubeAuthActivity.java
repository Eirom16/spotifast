package dev.eirom.doremi;

import android.app.Activity;
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
    private final Handler handler=new Handler(Looper.getMainLooper());
    private WebView browser;
    private boolean completed=false;
    private long generation;
    private final Runnable timeout=() -> complete("",2);
    private final Runnable poll=new Runnable() {
        @Override public void run() {
            if(completed || browser==null) return;
            // Google may leave its confirmation page visible after setting
            // the service cookies. Query only Music's origin, independently
            // of the visible page; Rust still verifies the musical account.
            String cookie=CookieManager.getInstance().getCookie(MUSIC);
            if(cookie!=null && cookie.length()>65536) {complete("",5);return;}
            if(sessionReady(cookie)) {
                complete(cookie,0);return;
            }
            handler.postDelayed(this,500);
        }
    };
    static boolean sessionReady(String cookie) {
        return cookie!=null && cookie.length()<=65536 && has(cookie,"LOGIN_INFO") &&
            (has(cookie,"SAPISID") || has(cookie,"__Secure-3PAPISID"));
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
                @Override public boolean shouldOverrideUrlLoading(WebView view,WebResourceRequest request) {
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
                // Let YouTube Music initiate its own account/return flow, as
                // Doremi-Rust does, instead of constructing a Google login URL.
                if(!completed && browser!=null) {browser.loadUrl(MUSIC);handler.post(poll);}
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
