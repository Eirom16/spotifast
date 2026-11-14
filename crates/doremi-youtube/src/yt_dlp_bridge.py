"""Private stdin/stdout adapter to the installed yt-dlp, not a YouTube extractor.

Credentials stay in memory. Only a selected AAC grant crosses the private pipe.
No CLI configs, user plugins, remote components or raw diagnostics are exposed.
"""
import contextlib
import http.cookiejar
import json
import re
import sys
import time


class Discard:
    def write(self, _text):
        return 0

    def flush(self):
        pass


class Diagnostics:
    def __init__(self):
        self.denied = False
        self.limited = False
        self.timeout = False
        self.unsupported = False

    def record(self, message):
        text = message.lower()
        self.denied |= bool(re.search(r"http(?: error)? (?:401|403)\b", text))
        self.denied |= any(term in text for term in (
            "sign in to confirm", "age-restricted", "not available in your country",
            "private video", "members-only",
        ))
        self.limited |= bool(re.search(r"http(?: error)? 429\b", text))
        self.timeout |= "timed out" in text
        self.unsupported |= any(term in text for term in (
            "requested format is not available", "no supported javascript runtime",
            "challenge solving failed",
        ))

    debug = record
    warning = record
    error = record

    def failure(self):
        if self.denied:
            return "access_denied"
        if self.limited:
            return "rate_limited"
        if self.timeout:
            return "timeout"
        if self.unsupported:
            return "unsupported_format"
        return "unavailable"


def resolve(request, diagnostics):
    import yt_dlp
    import yt_dlp.version
    from yt_dlp.globals import plugin_dirs

    version = re.match(r"(\d{4})\.(\d{2})\.(\d{2})", yt_dlp.version.__version__)
    if not version or tuple(map(int, version.groups())) < (2026, 10, 26):
        return {"status": "unsupported"}
    plugin_dirs.value = []
    local = request["id"]
    if request.get("schema") != 1 or not re.fullmatch(r"[A-Za-z0-9_-]{11}", local):
        return {"status": "invalid_request"}
    session = request.get("session") or {}
    options = {
        "quiet": True,
        "logger": diagnostics,
        "cachedir": False,
        "noplaylist": True,
        "skip_download": True,
        "socket_timeout": 15,
        "retries": 0,
        "extractor_retries": 0,
        "js_runtimes": {"deno": {}},
        "remote_components": set(),
        "format": "bestaudio[ext=m4a][protocol=https][acodec=mp4a.40.2]",
        # Use yt-dlp's normal extraction policy from the outset. Never fall
        # back here after a refusal from Doremi's experimental native resolver.
        "extractor_args": {"youtube": {"player_client": ["default"]}},
        "http_headers": {"X-Goog-AuthUser": session.get("auth_user", "0")},
    }
    for field in ("visitor_data", "data_sync_id"):
        if session.get(field):
            options["extractor_args"]["youtube"][field] = [session[field]]
    with yt_dlp.YoutubeDL(options) as extractor:
        for item in session.get("cookie", "").split(";"):
            name, separator, value = item.strip().partition("=")
            if separator:
                # Never forward an account Cookie header to googlevideo.
                extractor.cookiejar.set_cookie(http.cookiejar.Cookie(
                    0, name, value, None, False, ".youtube.com", True, True,
                    "/", True, True, int(time.time()) + 3600, False, None, None, {},
                ))
        info = extractor.extract_info(
            "https://music.youtube.com/watch?v=" + local, download=False,
        )
        # Do not accept another client's result after an explicit access refusal.
        if diagnostics.denied or diagnostics.limited:
            return {"status": diagnostics.failure()}
        if (not info or info.get("id") != local or info.get("has_drm")
                or info.get("protocol") != "https"
                or info.get("ext") != "m4a" or info.get("acodec") != "mp4a.40.2"
                or info.get("vcodec") != "none"):
            return {"status": "unsupported_format"}
        return {
            "status": "ok",
            "id": local,
            "url": info["url"],
            "length": info.get("filesize"),
            "duration_ms": round(info["duration"] * 1000),
            "user_agent": info.get("http_headers", {}).get("User-Agent", ""),
        }


def main():
    diagnostics = Diagnostics()
    try:
        encoded = sys.stdin.buffer.read(128 * 1024 + 1)
        if len(encoded) > 128 * 1024:
            reply = {"status": "too_large"}
        else:
            request = json.loads(encoded)
            # Third-party libraries must not print URLs, cookies or raw errors
            # into the host's diagnostics. The sole output is the private reply.
            with contextlib.redirect_stdout(Discard()), contextlib.redirect_stderr(Discard()):
                reply = resolve(request, diagnostics)
    except ImportError:
        reply = {"status": "unsupported"}
    except Exception as error:
        diagnostics.record(str(error))
        reply = {"status": diagnostics.failure()}
    encoded = json.dumps(reply, separators=(",", ":"))
    if len(encoded.encode()) > 64 * 1024:
        encoded = '{"status":"too_large"}'
    sys.stdout.write(encoded)
    sys.stdout.flush()


if __name__ == "__main__":
    main()
