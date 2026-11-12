//! Apply the public player's URL transforms only to an already authorized,
//! non-DRM format. No browser, host bindings, client retries or session in JS.
use crate::{Client, Error, stream::media_url};
use reqwest::Url;
use rquickjs::{Context, Function, Object, Runtime};
use serde_json::Value;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const LIMIT: usize = 4096;
const BUDGET: Duration = Duration::from_secs(8);
const HEAP: usize = 64 * 1024 * 1024;
const STACK: usize = 512 * 1024;
// A cancelled parser can finish its current Rust parse before it observes the
// stop flag. Keep its permit until that thread exits; rapid skips cannot create
// an unbounded set of parsers or JavaScript heaps.
static WORK: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);

// Deliberately no Debug: challenges and results are parts of signed media URLs.
struct Inputs {
    url: Url,
    signature: Option<(String, String)>,
    n: Option<String>,
}
struct Answers {
    code: Arc<str>,
    signature: Option<String>,
    n: Option<String>,
}
struct Stop(Arc<AtomicBool>);
impl Drop for Stop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

fn parameter(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= LIMIT
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.=".contains(&b))
}

fn inputs(format: &Value) -> Result<Inputs, Error> {
    let (url, signature) = if let Some(url) = format["url"].as_str() {
        (media_url(url)?, None)
    } else {
        let cipher = format["signatureCipher"]
            .as_str()
            .or_else(|| format["cipher"].as_str())
            .ok_or(Error::UnsupportedFormat)?;
        if cipher.len() > 64 * 1024 {
            return Err(Error::TooLarge);
        }
        let mut envelope =
            Url::parse("https://invalid.local/").map_err(|_| Error::InvalidResponse)?;
        envelope.set_query(Some(cipher));
        let mut url = None;
        let mut signature = None;
        let mut name = None;
        for (key, value) in envelope.query_pairs() {
            let slot = match key.as_ref() {
                "url" => &mut url,
                "s" => &mut signature,
                "sp" => &mut name,
                _ => continue,
            };
            if slot.replace(value.into_owned()).is_some() {
                return Err(Error::InvalidResponse);
            }
        }
        let name = name.unwrap_or_else(|| "signature".into());
        if !matches!(name.as_str(), "sig" | "signature" | "lsig") {
            return Err(Error::InvalidResponse);
        }
        let signature = signature
            .filter(|s| parameter(s))
            .ok_or(Error::InvalidResponse)?;
        (
            media_url(&url.ok_or(Error::InvalidResponse)?)?,
            Some((name, signature)),
        )
    };
    let mut n = None;
    for (key, value) in url.query_pairs() {
        if key == "n" && n.replace(value.into_owned()).is_some() {
            return Err(Error::InvalidResponse);
        }
    }
    if n.as_ref().is_some_and(|s| !parameter(s)) {
        return Err(Error::InvalidResponse);
    }
    Ok(Inputs { url, signature, n })
}

impl Client {
    pub(crate) async fn resolve_format_url(&mut self, format: &Value) -> Result<Url, Error> {
        let input = inputs(format)?;
        if input.signature.is_none() && input.n.is_none() {
            return Ok(input.url);
        }
        let config = self
            .player_config
            .as_ref()
            .ok_or(Error::UnsupportedFormat)?;
        let code = config.code.clone();
        let prepared = config.prepared;
        let signature = input.signature.as_ref().map(|(_, value)| value.clone());
        let n = input.n.clone();
        let permit = WORK.acquire().await.map_err(|_| Error::Unavailable)?;
        let stop = Stop(Arc::new(AtomicBool::new(false)));
        let cancelled = stop.0.clone();
        let answers = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            solve(
                code,
                prepared,
                signature.as_deref(),
                n.as_deref(),
                cancelled,
                BUDGET,
            )
        })
        .await
        .map_err(|_| Error::UnsupportedFormat)??;
        // Keep one preprocessed public player, never resolved URLs or answers.
        if let Some(config) = self.player_config.as_mut() {
            config.code = answers.code.clone();
            config.prepared = true;
        }
        finalize(input, answers)
    }
}

fn solve(
    code: Arc<str>,
    prepared: bool,
    signature: Option<&str>,
    n: Option<&str>,
    cancelled: Arc<AtomicBool>,
    budget: Duration,
) -> Result<Answers, Error> {
    if code.len() > crate::MAX_BYTES {
        return Err(Error::TooLarge);
    }
    let deadline = Instant::now() + budget;
    let stopped = || cancelled.load(Ordering::Acquire) || Instant::now() >= deadline;
    if stopped() {
        return Err(Error::Timeout);
    }
    let code: Arc<str> = if prepared {
        code
    } else {
        crate::player_program::prepare(&code)?.into()
    };
    if stopped() {
        return Err(Error::Timeout);
    }
    if code.len() > crate::MAX_BYTES {
        return Err(Error::TooLarge);
    }
    let runtime = Runtime::new().map_err(|_| Error::UnsupportedFormat)?;
    runtime.set_memory_limit(HEAP);
    runtime.set_max_stack_size(STACK);
    let interrupted = cancelled.clone();
    runtime.set_interrupt_handler(Some(Box::new(move || {
        interrupted.load(Ordering::Acquire) || Instant::now() >= deadline
    })));
    let context = Context::full(&runtime).map_err(|_| Error::UnsupportedFormat)?;
    let result = context.with(|ctx| -> rquickjs::Result<_> {
        ctx.globals().set("_result", Object::new(ctx.clone())?)?;
        ctx.eval::<(), _>(code.as_bytes())?;
        let functions: Object = ctx.globals().get("_result")?;
        let call = |name: &str, input: Option<&str>| -> rquickjs::Result<Option<String>> {
            input
                .map(|value| {
                    let function: Function = functions.get(name)?;
                    function.call((value,))
                })
                .transpose()
        };
        Ok((call("sig", signature)?, call("n", n)?))
    });
    if stopped() {
        return Err(Error::Timeout);
    }
    let (signature, transformed_n) = result.map_err(|_| Error::UnsupportedFormat)?;
    if signature.as_ref().is_some_and(|s| !parameter(s))
        || transformed_n.as_ref().is_some_and(|s| {
            !parameter(s) || Some(s.as_str()) == n || s.starts_with("enhanced_except_")
        })
    {
        return Err(Error::UnsupportedFormat);
    }
    Ok(Answers {
        code,
        signature,
        n: transformed_n,
    })
}

fn finalize(mut input: Inputs, answers: Answers) -> Result<Url, Error> {
    let pairs: Vec<_> = input
        .url
        .query_pairs()
        .filter(|(key, _)| {
            key != "n"
                && !input
                    .signature
                    .as_ref()
                    .is_some_and(|(name, _)| key == name)
        })
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if pairs.len() > 128 {
        return Err(Error::TooLarge);
    }
    {
        let mut query = input.url.query_pairs_mut();
        query.clear().extend_pairs(pairs);
        if let Some((name, _)) = input.signature {
            query.append_pair(&name, &answers.signature.ok_or(Error::UnsupportedFormat)?);
        }
        if input.n.is_some() {
            query.append_pair("n", &answers.n.ok_or(Error::UnsupportedFormat)?);
        }
    }
    media_url(input.url.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cancellation() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }
    #[test]
    fn signatures_and_throttling_use_only_bounded_ecmascript() {
        let code = r#"if (typeof fetch !== 'undefined' || typeof require !== 'undefined' || typeof std !== 'undefined' || typeof process !== 'undefined') throw Error(); _result.sig=s=>s.split('').reverse().join(''); _result.n=s=>s+'X';"#;
        let answers = solve(
            code.into(),
            true,
            Some("fixture"),
            Some("abc"),
            cancellation(),
            BUDGET,
        )
        .unwrap();
        let input = inputs(&json!({"signatureCipher":"s=fixture&sp=sig&url=https%3A%2F%2Frr.googlevideo.com%2Fvideoplayback%3Fitag%3D140%26n%3Dabc"})).unwrap();
        let url = finalize(input, answers).unwrap();
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(url.host_str(), Some("rr.googlevideo.com"));
        assert_eq!(pairs.get("sig").map(|s| s.as_ref()), Some("erutxif"));
        assert_eq!(pairs.get("n").map(|s| s.as_ref()), Some("abcX"));
        assert_eq!(pairs.get("itag").map(|s| s.as_ref()), Some("140"));
    }
    #[test]
    fn runtime_limits_cancel_and_reject_unsolved_results() {
        assert!(matches!(
            solve(
                "while(true){}".into(),
                true,
                None,
                None,
                cancellation(),
                Duration::from_millis(20)
            ),
            Err(Error::Timeout)
        ));
        let stop = cancellation();
        stop.store(true, Ordering::Release);
        assert!(matches!(
            solve("".into(), true, None, None, stop, BUDGET),
            Err(Error::Timeout)
        ));
        for code in [
            "_result.n=s=>s",
            "_result.n=s=>'enhanced_except_failed'",
            "_result.n=s=>'https://invalid/'",
            "new Array(50000000).fill('x')",
        ] {
            assert!(
                solve(
                    code.into(),
                    true,
                    None,
                    Some("fixture"),
                    cancellation(),
                    BUDGET
                )
                .is_err()
            );
        }
    }
    #[test]
    fn malformed_ciphers_and_untrusted_hosts_never_reach_javascript() {
        for value in [
            json!({"url":"https://evil.invalid/?n=abc"}),
            json!({"url":"https://rr.googlevideo.com/?n=a&n=b"}),
            json!({"signatureCipher":"s=a&s=b&url=https%3A%2F%2Frr.googlevideo.com"}),
            json!({"signatureCipher":"s=a&sp=redirect&url=https%3A%2F%2Frr.googlevideo.com"}),
        ] {
            assert!(inputs(&value).is_err());
        }
    }
    #[test]
    fn shared_player_program_keeps_tables_and_excludes_browser_startup() {
        // Original fixture for the public URL-object interface. It deliberately
        // needs a table initializer and the inert location supplied by EJS.
        let source = r#"var _yt_player={};(function(g){
            var window=this;
            for(var table=[],i=0;i<3;i++)table.push(i);
            class Format {
                constructor(values){this.values=values;}
                apply(){
                    if(!window.location.hostname)throw Error();
                    if(this.values.s)this.values.s=this.values.s.split('').reverse().join('');
                    if(this.values.n)this.values.n+=table.length;
                }
                get(key){return this.values[key];}
                set(key,value){this.values[key]=value;}
                clone(){return new Format(this.values);}
            }
            function factory(url,key,value){
                if(false)model.set('alr','yes');
                return new Format({[key]:value});
            }
            forbiddenBrowserStartup();
        })(_yt_player);"#;
        let answers = solve(
            source.into(),
            false,
            Some("fixture"),
            Some("abc"),
            cancellation(),
            BUDGET,
        )
        .unwrap();
        assert_eq!(answers.signature.as_deref(), Some("erutxif"));
        assert_eq!(answers.n.as_deref(), Some("abc3"));
        let warm = solve(
            answers.code.clone(),
            true,
            Some("fixture"),
            Some("abc"),
            cancellation(),
            BUDGET,
        )
        .unwrap();
        assert_eq!(answers.signature, warm.signature);
        assert_eq!(answers.n, warm.n);
    }
    #[tokio::test]
    async fn cancellation_retires_the_interpreter_and_allows_the_next_track() {
        let mut first = Client::new().unwrap();
        first.player_config = Some(crate::player_config::PlayerConfig {
            timestamp: 20760,
            loaded: Instant::now(),
            code: "_result.n=s=>{while(true){}}".into(),
            prepared: true,
        });
        let task = tokio::spawn(async move {
            first
                .resolve_format_url(&json!({"url":"https://rr.googlevideo.com/?n=abc"}))
                .await
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let mut next = Client::new().unwrap();
        next.player_config = Some(crate::player_config::PlayerConfig {
            timestamp: 20760,
            loaded: Instant::now(),
            code: "_result.n=s=>s+'X'".into(),
            prepared: true,
        });
        let result = tokio::time::timeout(
            Duration::from_secs(2),
            next.resolve_format_url(&json!({"url":"https://rr.googlevideo.com/?n=abc"})),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(
            result.query_pairs().find(|(key, _)| key == "n").unwrap().1,
            "abcX"
        );
    }
    #[test]
    #[ignore = "requires an explicitly supplied public Google player script; no credentials"]
    fn published_player_transforms_synthetic_values() {
        let path = std::env::var_os("DOREMI_TEST_PLAYER_JS")
            .expect("supply the public player script path");
        let source = std::fs::read_to_string(path).unwrap();
        let answer = solve(source.into(), false, Some("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"), Some("0123456789abcdef"), cancellation(), BUDGET).unwrap();
        assert!(answer.signature.is_some() && answer.n.is_some());
        let cached = solve(answer.code.clone(), true, Some("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"), Some("0123456789abcdef"), cancellation(), BUDGET).unwrap();
        assert_eq!(answer.signature, cached.signature);
        assert_eq!(answer.n, cached.n);
    }
}
