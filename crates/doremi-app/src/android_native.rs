//! Android EGL host for egui. GLSurfaceView owns the GL thread and lifecycle;
//! only that thread accesses the renderer. The retained Rust model has one owner.
use super::*;
use egui_glow::{Painter, ShaderVersion, glow};
use jni::{
    JNIEnv, JavaVM,
    objects::{GlobalRef, JClass, JObject, JString, JValue},
    sys::{jboolean, jfloat, jint, jlong, jstring},
};
use std::{
    cell::RefCell,
    ffi::{CString, c_char, c_void},
    sync::{Arc, OnceLock},
    time::Instant,
};

#[link(name = "EGL")]
unsafe extern "C" {
    fn eglGetProcAddress(name: *const c_char) -> *const c_void;
}

struct Renderer {
    ctx: egui::Context,
    app: DoremiApp,
    painter: Painter,
    events: Vec<egui::Event>,
    started: Instant,
    text_generation: jlong,
}
static APPLICATION: OnceLock<GlobalRef> = OnceLock::new();
static VM: OnceLock<JavaVM> = OnceLock::new();
static ACTIVITY_CLASS: OnceLock<GlobalRef> = OnceLock::new();
static DATA_DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
thread_local! { static RENDERER: RefCell<Option<Renderer>> = const { RefCell::new(None) }; }

// Java queues this intent on the existing GL owner; the main thread never waits.
#[unsafe(export_name = "Java_dev_eirom_doremi_MainActivity_nativeBack")]
pub extern "system" fn native_back(_: JNIEnv<'_>, _: JClass<'_>) {
    RENDERER.with(|cell| {
        if let Some(renderer) = cell.borrow_mut().as_mut() {
            renderer.app.go_back();
            renderer.ctx.request_repaint();
        }
    });
}

#[unsafe(export_name = "Java_dev_eirom_doremi_MainActivity_nativeInit")]
pub extern "system" fn native_init(
    mut env: JNIEnv<'_>,
    class: JClass<'_>,
    directory: JString<'_>,
    context: JObject<'_>,
    input_generation: jlong,
) -> jboolean {
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("Doremi"),
    );
    let Ok(directory) = env.get_string(&directory) else {
        return 0;
    };
    let directory = PathBuf::from(String::from(directory));
    // A retained model/writer belongs to exactly one private data profile.
    // Switching QA profiles requires an explicit process restart by the harness.
    if DATA_DIRECTORY.get_or_init(|| directory.clone()) != &directory {
        log::error!("Doremi data profile cannot change within the same process");
        return 0;
    }
    let Ok(vm) = env.get_java_vm() else {
        return 0;
    };
    let Ok(context) = env.new_global_ref(context) else {
        return 0;
    };
    APPLICATION.get_or_init(|| {
        // The application GlobalRef and JavaVM remain valid for process lifetime.
        // This singleton is initialized once, before starting the AAudio worker.
        unsafe {
            ndk_context::initialize_android_context(
                vm.get_java_vm_pointer().cast(),
                context.as_obj().as_raw().cast(),
            );
        }
        context
    });
    VM.get_or_init(|| vm);
    if let Ok(class) = env.new_global_ref(class) {
        ACTIVITY_CLASS.get_or_init(|| class);
    }
    RENDERER.with(|cell| {
        if cell.borrow().is_some() {
            return 1;
        }
        // GLSurfaceView invokes this with a current GLES context. EGL returns
        // process-lifetime function pointers, used only on this same GL thread.
        let gl = unsafe {
            glow::Context::from_loader_function(|name| {
                let name = CString::new(name).expect("GL symbol");
                eglGetProcAddress(name.as_ptr())
            })
        };
        let painter = match Painter::new(Arc::new(gl), "", Some(ShaderVersion::Es300), false) {
            Ok(painter) => painter,
            Err(error) => {
                log::error!("Doremi renderer: {error}");
                return 0;
            }
        };
        let ctx = egui::Context::default();
        let mut app = DoremiApp::new_context(&ctx, directory);
        app.model.as_mut().unwrap().music.session.restore();
        *cell.borrow_mut() = Some(Renderer {
            ctx,
            app,
            painter,
            events: vec![],
            started: Instant::now(),
            text_generation: input_generation,
        });
        1
    })
}

#[unsafe(export_name = "Java_dev_eirom_doremi_MainActivity_nativeDraw")]
pub extern "system" fn native_draw(
    env: JNIEnv<'_>,
    _: JClass<'_>,
    width: jint,
    height: jint,
    scale: jfloat,
) -> jstring {
    let output = RENDERER.with(|cell| {
        let mut held = cell.borrow_mut();
        let Some(renderer) = held.as_mut() else { return "{}".to_owned(); };
        if width <= 0 || height <= 0 || scale <= 0.0 { return "{}".to_owned(); }
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width as f32 / scale,height as f32 / scale))),
            time: Some(renderer.started.elapsed().as_secs_f64()),
            events: std::mem::take(&mut renderer.events), focused: true,
            ..Default::default()
        };
        input.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(scale);
        let mut output = renderer.ctx.run_ui(input, |ui| renderer.app.draw(ui));
        let primitives = renderer.ctx.tessellate(output.shapes, output.pixels_per_point);
        renderer.painter.clear([width as u32,height as u32],[0.06,0.05,0.08,1.0]);
        renderer.painter.paint_and_update_textures([width as u32,height as u32], output.pixels_per_point,&primitives,&mut output.textures_delta);
        let delay = output.viewport_output.get(&egui::ViewportId::ROOT).map(|v| v.repaint_delay.as_millis().min(10_000) as u64).unwrap_or(10_000);
        serde_json::json!({"ime":output.platform_output.ime.is_some(), "can_go_back":renderer.app.can_go_back(), "query":renderer.app.model.as_ref().unwrap().query, "editor_target":renderer.app.editor.key(), "editor_text":renderer.app.editor_text(), "input_generation":renderer.text_generation, "file_request":renderer.app.model.as_ref().unwrap().local_files.selector.unwrap_or(0), "session_request":renderer.app.model.as_ref().unwrap().music.session.picker_request(), "needs_focus":renderer.app.model.as_ref().unwrap().needs_focus(), "loading_audio":renderer.app.model.as_ref().unwrap().real_audio() && renderer.app.model.as_ref().unwrap().queue.playback() == Playback::Loading, "next_ms":delay}).to_string()
    });
    env.new_string(output)
        .map_or(std::ptr::null_mut(), |text| text.into_raw())
}

#[unsafe(export_name = "Java_dev_eirom_doremi_MainActivity_nativeTouch")]
pub extern "system" fn native_touch(
    _: JNIEnv<'_>,
    _: JClass<'_>,
    action: jint,
    x: jfloat,
    y: jfloat,
) {
    RENDERER.with(|cell| {
        let mut held = cell.borrow_mut();
        let Some(renderer) = held.as_mut() else {
            return;
        };
        let pos = egui::pos2(x, y);
        let phase = match action {
            0 => egui::TouchPhase::Start,
            1 => egui::TouchPhase::End,
            3 => egui::TouchPhase::Cancel,
            _ => egui::TouchPhase::Move,
        };
        renderer.events.push(egui::Event::Touch {
            device_id: egui::TouchDeviceId(0),
            id: egui::TouchId(0),
            phase,
            pos,
            force: None,
        });
        renderer.events.push(egui::Event::PointerMoved(pos));
        if action == 0 || action == 1 || action == 3 {
            renderer.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: action == 0,
                modifiers: egui::Modifiers::default(),
            });
        }
        if action == 1 || action == 3 {
            renderer.events.push(egui::Event::PointerGone);
        }
    });
}

#[unsafe(export_name = "Java_dev_eirom_doremi_MainActivity_nativeText")]
pub extern "system" fn native_text(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    text: JString<'_>,
    field: JString<'_>,
    generation: jlong,
) {
    let Ok(text) = env.get_string(&text) else {
        return;
    };
    let text = String::from(text);
    let Ok(field) = env.get_string(&field) else {
        return;
    };
    let field = String::from(field);
    RENDERER.with(|cell| {
        if let Some(renderer) = cell.borrow_mut().as_mut() {
            if generation < renderer.text_generation {
                return;
            }
            renderer.text_generation = generation;
            renderer.app.receive_editor_text(&field, &text);
        }
    });
}

#[unsafe(export_name = "Java_dev_eirom_doremi_MainActivity_nativeDestroy")]
pub extern "system" fn native_destroy(_: JNIEnv<'_>, _: JClass<'_>) {
    RENDERER.with(|cell| {
        if let Some(mut renderer) = cell.borrow_mut().take() {
            // Called on the GL thread while its context is still current.
            renderer.painter.destroy();
        }
    });
}

#[unsafe(export_name = "Java_dev_eirom_doremi_MainActivity_nativeFocus")]
pub extern "system" fn native_focus(_: JNIEnv<'_>, _: JClass<'_>, granted: jboolean) {
    RENDERER.with(|cell| {
        if let Some(renderer) = cell.borrow_mut().as_mut() {
            let model = renderer.app.model.as_mut().unwrap();
            model.focus = granted != 0;
            if !model.focus
                && model.real_audio()
                && matches!(
                    model.queue.playback(),
                    Playback::Playing | Playback::Loading
                )
            {
                if model.queue.playback() == Playback::Loading && model.online.phase() != "idle" {
                    model.command_with_import_policy(Command::Cancel, false);
                } else {
                    model.command(Command::Pause);
                }
                model.notice =
                    "Audio en pausa: foco no disponible o aplicación en segundo plano.".into();
            } else if let Some(engine) = &model.audio {
                engine.set_playing(model.needs_focus() && model.focus);
            }
        }
    });
}

/// Only the file worker calls these Android APIs. No URI is logged or persisted.
pub fn open_document(uri: &str) -> Result<(std::fs::File, String), ()> {
    use std::os::fd::FromRawFd;
    let mut env = VM
        .get()
        .ok_or(())?
        .attach_current_thread()
        .map_err(|_| ())?;
    let class = ACTIVITY_CLASS.get().ok_or(())?;
    let class: &JClass<'_> = class.as_obj().into();
    let context = APPLICATION.get().ok_or(())?.as_obj();
    let result = (|| -> jni::errors::Result<_> {
        let uri = env.new_string(uri)?;
        let args = [JValue::Object(context), JValue::Object(uri.as_ref())];
        let parcel = env
            .call_static_method(
                class,
                "openDocument",
                "(Landroid/content/Context;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;",
                &args,
            )?
            .l()?;
        let fd = env
            .call_method(&parcel, "detachFd", "()I", &[])
            .and_then(|v| v.i());
        // detachFd transfers ownership exactly once to Rust. Close the Java
        // wrapper regardless of success, clearing an exception before closing.
        if fd.is_err() {
            let _ = env.exception_clear();
        }
        let _ = env.call_method(&parcel, "close", "()V", &[]);
        let fd = fd?;
        if fd < 0 {
            return Err(jni::errors::Error::NullPtr("document descriptor"));
        }
        // SAF supplies a read-only owned descriptor, never a borrowed Java fd.
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        let title = env
            .call_static_method(
                class,
                "documentTitle",
                "(Landroid/content/Context;Ljava/lang/String;)Ljava/lang/String;",
                &args,
            )?
            .l()?;
        let title = String::from(env.get_string(&JString::from(title))?);
        Ok((file, title))
    })();
    if result.is_err() {
        let _ = env.exception_clear();
    }
    result.map_err(|_| ())
}

#[unsafe(export_name = "Java_dev_eirom_doremi_MainActivity_nativeDocument")]
pub extern "system" fn native_document(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    generation: jlong,
    uri: JString<'_>,
) {
    let Ok(uri) = env.get_string(&uri) else {
        return;
    };
    let uri = String::from(uri);
    RENDERER.with(|cell| {
        if let Some(renderer) = cell.borrow_mut().as_mut() {
            renderer.app.model.as_mut().unwrap().selected_document(
                generation as u64,
                uri,
                &renderer.ctx,
            );
        }
    });
}

/// JNI carries platform capabilities only. It never validates a musical account.
pub(super) mod youtube_credentials {
    use super::*;
    use doremi_core::catalog::ProviderError as Error;
    enum Operation<'a> {
        Load,
        Store(&'a str),
        Forget,
        Document(&'a str),
    }
    fn call(operation: Operation<'_>) -> Result<Option<Vec<u8>>, Error> {
        let mut env = VM
            .get()
            .ok_or(Error::Unavailable)?
            .attach_current_thread()
            .map_err(|_| Error::Unavailable)?;
        let class = ACTIVITY_CLASS.get().ok_or(Error::Unavailable)?;
        let class: &JClass<'_> = class.as_obj().into();
        let context = APPLICATION.get().ok_or(Error::Unavailable)?.as_obj();
        let directory = DATA_DIRECTORY
            .get()
            .and_then(|directory| directory.to_str())
            .ok_or(Error::Unavailable)?;
        let result = (|| -> jni::errors::Result<_> {
            let argument = env.new_string(match operation {
                Operation::Document(uri) => uri,
                _ => directory,
            })?;
            let secret = env.byte_array_from_slice(match operation {
                Operation::Store(cookie) => cookie.as_bytes(),
                _ => &[],
            })?;
            let mut args = vec![JValue::Object(context), JValue::Object(argument.as_ref())];
            let (method, signature, bytes) = match operation {
                Operation::Load => (
                    "loadYoutubeSession",
                    "(Landroid/content/Context;Ljava/lang/String;)[B",
                    true,
                ),
                Operation::Store(_) => {
                    args.push(JValue::Object(secret.as_ref()));
                    (
                        "storeYoutubeSession",
                        "(Landroid/content/Context;Ljava/lang/String;[B)V",
                        false,
                    )
                }
                Operation::Forget => (
                    "forgetYoutubeSession",
                    "(Landroid/content/Context;Ljava/lang/String;)V",
                    false,
                ),
                Operation::Document(_) => (
                    "readYoutubeSessionDocument",
                    "(Landroid/content/Context;Ljava/lang/String;)[B",
                    true,
                ),
            };
            let result = env.call_static_method(class, method, signature, &args)?;
            if !bytes {
                return Ok(None);
            }
            let value = result.l()?;
            if value.is_null() {
                return Ok(None);
            }
            env.convert_byte_array(jni::objects::JByteArray::from(value))
                .map(Some)
        })();
        if result.is_err() {
            let exception = env.exception_occurred().ok();
            let _ = env.exception_clear();
            if let Some(exception) = exception.filter(|exception| !exception.is_null()) {
                if env
                    .is_instance_of(&exception, "java/io/InterruptedIOException")
                    .unwrap_or(false)
                {
                    return Err(Error::Timeout);
                }
                if env
                    .is_instance_of(&exception, "java/lang/IllegalArgumentException")
                    .unwrap_or(false)
                {
                    return Err(Error::TooLarge);
                }
            }
        }
        result.map_err(|_| Error::Unavailable)
    }
    pub fn load() -> Result<Option<String>, Error> {
        call(Operation::Load)?
            .map(|bytes| String::from_utf8(bytes).map_err(|_| Error::InvalidResponse))
            .transpose()
    }
    pub fn store(cookie: &str) -> Result<(), Error> {
        call(Operation::Store(cookie)).map(|_| ())
    }
    pub fn forget() -> Result<(), Error> {
        call(Operation::Forget).map(|_| ())
    }
    pub fn read_document(uri: &str) -> Result<Vec<u8>, Error> {
        call(Operation::Document(uri))?.ok_or(Error::InvalidResponse)
    }
}

#[unsafe(export_name = "Java_dev_eirom_doremi_MainActivity_nativeSessionDocument")]
pub extern "system" fn native_session_document(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    generation: jlong,
    uri: JString<'_>,
) {
    let Ok(uri) = env.get_string(&uri) else {
        return;
    };
    let uri = String::from(uri);
    RENDERER.with(|cell| {
        if let Some(renderer) = cell.borrow_mut().as_mut() {
            renderer
                .app
                .model
                .as_mut()
                .unwrap()
                .music
                .session
                .selected_document(generation as u64, uri);
            renderer.ctx.request_repaint();
        }
    });
}
