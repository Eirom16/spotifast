#!/usr/bin/env python3
"""Test Android's real document selector using only Doremi's original CC0 WAV.

No storage permission, personal files, global log or system-picker screenshots.
The APK is verified and updated in place; existing Doremi data is preserved.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import runpy
import shlex
import subprocess
import uuid
import wave

ROOT=Path(__file__).resolve().parents[1]
PACKAGE="dev.eirom.doremi.dev"

def adb(*args):
    return subprocess.check_output(["adb","-d",*args],timeout=240)

def digest(data):return hashlib.sha256(data).hexdigest()

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact",type=Path)
    parser.add_argument("--commit",required=True)
    parser.add_argument("--attempt",default="",help="New evidence suffix after a diagnosed terminal failure")
    args=parser.parse_args()
    apk=runpy.run_path(str(ROOT/"scripts/android-qa.py"))["verify_payload"](args.artifact,args.commit)
    assert re.fullmatch(r"[a-z0-9-]{0,32}",args.attempt)
    out=ROOT/"target/android-qa"/args.commit[:12]/("documents"+("-"+args.attempt if args.attempt else ""))
    out.mkdir(parents=True,exist_ok=False)
    source=ROOT/"assets/audio/scale.wav"
    manifest=json.loads((source.parent/"manifest.json").read_text())
    expected=next(item for item in manifest["files"] if item["file"]=="scale.wav")
    assert manifest["license"]=="CC0-1.0" and digest(source.read_bytes())==expected["sha256"]
    with wave.open(str(source),"rb") as wav:
        assert (wav.getnchannels(),wav.getsampwidth(),wav.getframerate())==(1,2,48000)
        original=wav.readframes(wav.getnframes())
    fixture=out/"Original Café estéreo.wav"
    with wave.open(str(fixture),"wb") as wav:
        wav.setnchannels(2);wav.setsampwidth(2);wav.setframerate(16000)
        wav.writeframes(b"".join(original[i:i+2]*2 for i in range(0,len(original),6))*15)
    # A well-formed 24-bit container remains visible under Android's audio MIME
    # filter but is explicitly unsupported by the first PCM16 import slice.
    invalid=out/"Invalid.wav"
    with wave.open(str(invalid),"wb") as wav:
        wav.setnchannels(1);wav.setsampwidth(3);wav.setframerate(16000);wav.writeframes(bytes(480))
    folder="Doremi-QA-"+args.commit[:12]+uuid.uuid4().hex[:8]
    remote="/sdcard/Download/"+folder
    exists=subprocess.run(["adb","-d","shell","test","-e",remote],capture_output=True,timeout=30)
    assert exists.returncode==1,"Never overwrite an existing device directory"
    adb("shell","mkdir",remote)
    for path in (fixture,invalid):adb("push",str(path),remote+"/"+path.name)
    expected_digest=digest(fixture.read_bytes())
    def original_intact():
        actual=adb("shell","sha256sum",shlex.quote(remote+"/"+fixture.name)).decode().split()[0]
        assert actual==expected_digest,"Owned original changed"
    original_intact()
    (out/"fixture.json").write_text(json.dumps(dict(source="assets/audio/scale.wav",license="CC0-1.0",source_sha256=expected["sha256"],fixture_sha256=expected_digest,device_directory=remote),indent=2)+"\n")
    adb("install","-r","-t",str(apk))
    installed=adb("shell","pm","path",PACKAGE).decode().strip()
    assert installed.startswith("package:") and "\n" not in installed
    assert digest(adb("exec-out","cat",installed.removeprefix("package:")))==digest(apk.read_bytes())
    adb("install","-r","-t",str(args.artifact/"app-debug-androidTest.apk"))
    device={key:adb("shell","getprop",key).decode().strip() for key in ("ro.product.manufacturer","ro.product.model","ro.build.version.sdk","ro.product.cpu.abilist")}
    pids=[]
    def run(mode,captures):
        evidence=out/mode;evidence.mkdir()
        adb("shell","am","force-stop",PACKAGE)
        log=adb("shell","am","instrument","-w","-e","mode",mode,"-e","documentFolder",folder,f"{PACKAGE}.test/dev.eirom.doremi.DoremiInstrumentation")
        (evidence/"instrumentation.log").write_bytes(log)
        latest=json.loads(adb("exec-out","run-as",PACKAGE,"cat","files/qa-state.json"))
        (evidence/"latest-state.json").write_text(json.dumps(latest,indent=2)+"\n")
        own_log=adb("logcat","-d",f"--pid={int(latest['pid'])}","-t","1000").decode(errors="replace")
        (evidence/"own-process.log").write_text(own_log)
        assert log.decode().startswith("PASS: ") and "FAIL:" not in log.decode(),log.decode()
        assert not re.search(r"FATAL EXCEPTION|Fatal signal \d+|panicked at|JNI DETECTED ERROR|shutdown.*timed out",own_log,re.IGNORECASE)
        for name in captures:
            for suffix in ("json","png"):
                data=adb("exec-out","run-as",PACKAGE,"cat",f"files/{name}.{suffix}")
                if suffix=="png":assert data.startswith(b"\x89PNG\r\n\x1a\n")
                else:assert json.loads(data)["pid"]==latest["pid"]
                (evidence/f"{name}.{suffix}").write_bytes(data)
        check_name="qa-document-import-checks.json" if mode=="document-import" else "qa-document-restore-checks.json"
        checks=json.loads(adb("exec-out","run-as",PACKAGE,"cat","files/"+check_name))
        assert checks["result"]=="pass"
        pids.append(latest["pid"])
        original_intact()
        return checks
    imported=run("document-import",("qa-document-invalid","qa-document-playing","qa-document-library-light","qa-document-library-dark","qa-document-recreated"))
    resources=json.loads(adb("exec-out","run-as",PACKAGE,"cat","files/qa-document-resources.json"))
    assert resources["current_provider"]=="local-files" and resources["audio_metrics"]["nonzero_samples"]>0
    (out/"document-resources.json").write_text(json.dumps(resources,indent=2)+"\n")
    restored=run("document-restore",("qa-document-restored","qa-document-restored-audio"))
    assert pids[0]!=pids[1] and imported["id"]==restored["id"]
    index=json.loads(adb("exec-out","run-as",PACKAGE,"cat","files/local-files-v1.json"))
    path=next(item["path"] for item in index["files"] if item["id"]==imported["id"])
    assert re.fullmatch(r"/data/(?:user/0|data)/dev\.eirom\.doremi\.dev/files/imported-documents/[0-9a-f]{64}/source\.wav",path)
    assert adb("exec-out","run-as",PACKAGE,"sha256sum",path).decode().split()[0]==expected_digest
    (out/"summary.json").write_text(json.dumps(dict(result="pass",source_commit=args.commit,physical_device=True,device=device,captures=7,fresh_process=True,original_preserved=True,private_copy_sha256=expected_digest,resources=resources,imported=imported,restored=restored,acoustic_capture=False),indent=2)+"\n")
    print(out)

if __name__=="__main__":main()
