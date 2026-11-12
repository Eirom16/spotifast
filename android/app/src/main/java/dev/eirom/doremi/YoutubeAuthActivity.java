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
    // Google-owned cookie-bridge domains: https://www.google.com/supported_domains
    // Snapshot 2026-11-12; never infer ownership from a google.* prefix.
    static final java.util.Set<String> GOOGLE_DOMAINS=java.util.Collections.unmodifiableSet(new java.util.HashSet<>(java.util.Arrays.asList((
        ".google.com .google.ad .google.ae .google.com.af .google.com.ag .google.al .google.am .google.co.ao .google.com.ar .google.as .google.at .google.com.au "
        + ".google.az .google.ba .google.com.bd .google.be .google.bf .google.bg .google.com.bh .google.bi .google.bj .google.com.bn .google.com.bo .google.com.br "
        + ".google.bs .google.bt .google.co.bw .google.by .google.com.bz .google.ca .google.cd .google.cf .google.cg .google.ch .google.ci .google.co.ck "
        + ".google.cl .google.cm .google.cn .google.com.co .google.co.cr .google.com.cu .google.cv .google.com.cy .google.cz .google.de .google.dj .google.dk "
        + ".google.dm .google.com.do .google.dz .google.com.ec .google.ee .google.com.eg .google.es .google.com.et .google.fi .google.com.fj .google.fm .google.fr "
        + ".google.ga .google.ge .google.gg .google.com.gh .google.com.gi .google.gl .google.gm .google.gr .google.com.gt .google.gy .google.com.hk .google.hn "
        + ".google.hr .google.ht .google.hu .google.co.id .google.ie .google.co.il .google.im .google.co.in .google.iq .google.is .google.it .google.je "
        + ".google.com.jm .google.jo .google.co.jp .google.co.ke .google.com.kh .google.ki .google.kg .google.co.kr .google.com.kw .google.kz .google.la .google.com.lb "
        + ".google.li .google.lk .google.co.ls .google.lt .google.lu .google.lv .google.com.ly .google.co.ma .google.md .google.me .google.mg .google.mk "
        + ".google.ml .google.com.mm .google.mn .google.com.mt .google.mu .google.mv .google.mw .google.com.mx .google.com.my .google.co.mz .google.com.na .google.com.ng "
        + ".google.com.ni .google.ne .google.nl .google.no .google.com.np .google.nr .google.nu .google.co.nz .google.com.om .google.com.pa .google.com.pe .google.com.pg "
        + ".google.com.ph .google.com.pk .google.pl .google.pn .google.com.pr .google.ps .google.pt .google.com.py .google.com.qa .google.ro .google.ru .google.rw "
        + ".google.com.sa .google.com.sb .google.sc .google.se .google.com.sg .google.sh .google.si .google.sk .google.com.sl .google.sn .google.so .google.sm "
        + ".google.sr .google.st .google.com.sv .google.td .google.tg .google.co.th .google.com.tj .google.tl .google.tm .google.tn .google.to .google.com.tr "
        + ".google.tt .google.com.tw .google.co.tz .google.com.ua .google.co.ug .google.co.uk .google.com.uy .google.co.uz .google.com.vc .google.co.ve .google.co.vi .google.com.vn "
        + ".google.vu .google.ws .google.rs .google.co.za .google.co.zm .google.co.zw .google.cat"
    ).split(" "))));
    private final Handler handler=new Handler(Looper.getMainLooper());
    private WebView browser;
    private boolean completed=false;
    private long generation;
    private boolean readingContext=false;
    private int contextAttempts=0;
    private long readGeneration=0,readDeadline=0;
    private int traceCount=0;
    private Boolean observedSession;
    private void trace(String event) {
        // Categories/codes and a rejected hostname only. No page text, full
        // URLs, signed parameters or credentials.
        if(traceCount++<64) android.util.Log.i("DoremiAuth",event);
    }
    private static String site(String value) {
        String host=value==null?null:Uri.parse(value).getHost();
        if(googleAccountHost(host)) return "google_account";
        if(host!=null && host.startsWith("www.") && GOOGLE_DOMAINS.contains("."+host.substring(4))) return "google_cookie_bridge";
        if("music.youtube.com".equals(host)) return "music";
        if("accounts.youtube.com".equals(host)) return "youtube_account";
        return "other";
    }
    private static boolean googleAccountHost(String host) {
        return host!=null && host.startsWith("accounts.") && GOOGLE_DOMAINS.contains("."+host.substring(9));
    }
    private final Runnable timeout=() -> complete("",2);
    private final Runnable poll=new Runnable() {
        @Override public void run() {
            if(completed || browser==null) return;
            String cookie=CookieManager.getInstance().getCookie(MUSIC);
            if(cookie!=null && cookie.length()>65536) {complete("",5);return;}
            boolean ready=sessionReady(cookie);
            if(observedSession==null || observedSession!=ready) {
                observedSession=ready;trace(ready?"music_session_present":"music_session_absent");
            }
            if(readingContext && android.os.SystemClock.uptimeMillis()>=readDeadline) readingContext=false;
            if(ready && musicPage(browser.getUrl()) && !readingContext) {
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
        if(intent==null) {trace("platform_handoff_rejected");complete("",4);return true;}
        String fallback=intent.getStringExtra("browser_fallback_url");intent.removeExtra("browser_fallback_url");
        try {startActivity(intent);trace("platform_handoff_started");}
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
        if(googleAccountHost(host)) return true;
        if(host.startsWith("www.") && GOOGLE_DOMAINS.contains("."+host.substring(4))) {
            String path=uri.getPath();
            return path!=null && (path.startsWith("/accounts/") || path.equals("/CheckCookie"));
        }
        return java.util.Arrays.asList("accounts.google.com","consent.google.com","myaccount.google.com",
            "music.youtube.com","www.youtube.com","youtube.com","accounts.youtube.com","consent.youtube.com").contains(host);
    }
    static boolean denied(String value) {
        Uri uri=Uri.parse(value);
        return googleAccountHost(uri.getHost()) &&
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
                    trace("page_started:"+site(url));
                    if(readGeneration==Long.MAX_VALUE) {complete("",4);return;}
                    readGeneration++;readingContext=false;contextAttempts=0;
                }
                @Override public boolean shouldOverrideUrlLoading(WebView view,WebResourceRequest request) {
                    if("intent".equals(request.getUrl().getScheme())) return handoff(view,request.getUrl().toString());
                    if(!request.isForMainFrame()) return false;
                    String url=request.getUrl().toString();
                    if(denied(url)) {complete("",3);return true;}
                    if(!allowed(url)) {
                        trace("navigation_rejected:"+site(url));
                        Uri target=request.getUrl();String host=target.getHost(),scheme=target.getScheme();
                        // Origin metadata only. Never include path, query, userinfo,
                        // fragments, error descriptions or the complete URL.
                        if(host==null || !host.matches("[A-Za-z0-9.-]{1,253}")) host="none";
                        if(!java.util.Arrays.asList("https","http","about","data").contains(scheme)) scheme="custom";
                        trace("rejected_authority:"+scheme+":"+host+":"+target.getPort());
                        complete("",4);return true;
                    }
                    return false;
                }
                @Override public void onPageFinished(WebView view,String url) {
                    trace("page_finished:"+site(url));
                    if(denied(url)) complete("",3);
                }
                @Override public void onReceivedError(WebView view,WebResourceRequest request,android.webkit.WebResourceError error) {
                    if(completed) return;
                    trace("network_error:"+error.getErrorCode()+":"+site(request.getUrl().toString()));
                    if(request.isForMainFrame()) complete("",error.getErrorCode()==WebViewClient.ERROR_TIMEOUT?2:4);
                }
                @Override public void onReceivedHttpError(WebView view,WebResourceRequest request,android.webkit.WebResourceResponse response) {
                    if(completed) return;
                    int code=response.getStatusCode();
                    trace("http_error:"+code+":"+site(request.getUrl().toString()));
                    if(request.isForMainFrame()) complete("",code==401 || code==403?3:4);
                }
                @Override public void onReceivedSslError(WebView view,android.webkit.SslErrorHandler decision,android.net.http.SslError error) {
                    trace("tls_error:"+error.getPrimaryError());decision.cancel();complete("",4);
                }
                @Override public boolean onRenderProcessGone(WebView view,android.webkit.RenderProcessGoneDetail detail) {
                    trace("renderer_gone");complete("",4);return true;
                }
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
        trace("completed:"+status);
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
