//! Android EGL host for egui. GLSurfaceView owns the GL thread and lifecycle;
//! only that thread accesses the renderer. The retained Rust model has one owner.
use super::*;
use egui_glow::{Painter, ShaderVersion, glow};
use jni::{
    JNIEnv,
    objects::{JClass, JString},
    sys::{jboolean, jfloat, jint, jstring},
};
use std::{
    cell::RefCell,
    ffi::{CString, c_char, c_void},
    sync::Arc,
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
}
thread_local! { static RENDERER: RefCell<Option<Renderer>> = const { RefCell::new(None) }; }

#[unsafe(export_name = "Java_dev_eirom_doremi_MainActivity_nativeInit")]
pub extern "system" fn native_init(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    directory: JString<'_>,
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
        let app = DoremiApp::new_context(&ctx, directory);
        *cell.borrow_mut() = Some(Renderer {
            ctx,
            app,
            painter,
            events: vec![],
            started: Instant::now(),
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
        serde_json::json!({"ime":output.platform_output.ime.is_some(), "query":renderer.app.model.as_ref().unwrap().query, "next_ms":delay}).to_string()
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
pub extern "system" fn native_text(mut env: JNIEnv<'_>, _: JClass<'_>, text: JString<'_>) {
    let Ok(text) = env.get_string(&text) else {
        return;
    };
    let text = String::from(text);
    RENDERER.with(|cell| {
        if let Some(renderer) = cell.borrow_mut().as_mut() {
            let model = renderer.app.model.as_mut().unwrap();
            if model.query != text {
                model.query = text.chars().take(128).collect();
                model.search();
            }
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
