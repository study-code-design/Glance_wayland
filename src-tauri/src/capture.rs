#[cfg(not(target_os = "macos"))]
use screenshots::Screen as CaptureScreen;

#[cfg(target_os = "macos")]
use std::{
    fs,
    io::Cursor,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "linux")]
use std::{
    env, fs,
    path::PathBuf,
    process::{Command, Output},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::error::{AppError, AppResult};

#[cfg(target_os = "macos")]
const DEBUG_CAPTURE_DIR: &str = "/tmp/glance-debug/latest";

/// Monitor info returned by find_cursor_monitor / find_primary_screen.
#[derive(Clone, Copy)]
pub struct MonitorInfo {
    pub scale_factor: f64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[cfg(target_os = "macos")]
    pub display_id: u32,
}

#[cfg(not(target_os = "macos"))]
pub struct CursorMonitorResult {
    pub screen: CaptureScreen,
    pub monitor: MonitorInfo,
}

#[cfg(target_os = "linux")]
pub struct LinuxWaylandCapture {
    pub png_bytes: Vec<u8>,
    pub rgba_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub rect_x: i32,
    pub rect_y: i32,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Debug)]
struct WaylandLaunchEnv {
    wayland_display: String,
    hyprland_instance_signature: Option<String>,
    xdg_runtime_dir: Option<PathBuf>,
}

#[cfg(target_os = "linux")]
#[derive(Debug, serde::Deserialize)]
struct HyprlandInstanceRecord {
    #[serde(default)]
    instance: String,
    #[serde(default)]
    #[serde(alias = "wlSocket", alias = "wl_socket")]
    wl_socket: String,
}

#[cfg(target_os = "macos")]
pub struct InteractiveCaptureImage {
    pub png_bytes: Vec<u8>,
    pub rgba_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[cfg(target_os = "macos")]
pub struct CapturedScreenImage {
    pub preview_bytes: Vec<u8>,
    pub preview_mime: &'static str,
    pub rgba_bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

// ── Cursor-aware monitor detection ──────────────────────────────────────────

/// Find the monitor that the cursor is currently on.
#[cfg(not(target_os = "macos"))]
pub fn find_cursor_monitor() -> AppResult<CursorMonitorResult> {
    let (cursor_x, cursor_y) = get_cursor_position()
        .map_err(|e| AppError::Capture(format!("failed to get cursor position: {e}")))?;

    // Screen::from_point finds the screen containing the given point
    let screen = CaptureScreen::from_point(cursor_x, cursor_y).map_err(|e| {
        AppError::Capture(format!("no screen at cursor ({cursor_x},{cursor_y}): {e}"))
    })?;

    let info = &screen.display_info;
    Ok(CursorMonitorResult {
        screen,
        monitor: MonitorInfo {
            scale_factor: info.scale_factor as f64,
            x: info.x,
            y: info.y,
            width: info.width,
            height: info.height,
        },
    })
}

/// Find the monitor that the cursor is currently on.
#[cfg(target_os = "macos")]
pub fn find_cursor_monitor() -> AppResult<MonitorInfo> {
    use core_graphics::display::CGDisplay;
    use core_graphics::geometry::CGPoint;

    let t0 = std::time::Instant::now();

    // Get cursor position via NSEvent (AppKit coords: lower-left origin, Y up).
    // CGDisplay::bounds() uses CG coords: upper-left origin, Y down.
    // We need to convert: cg_y = main_bounds.origin.y + main_bounds.size.height - ns_y
    let main_display = CGDisplay::main();
    let main_bounds = main_display.bounds();

    let ns_point = unsafe { objc2_app_kit::NSEvent::mouseLocation() };
    let cg_x = ns_point.x;
    let cg_y = main_bounds.origin.y + main_bounds.size.height - ns_point.y;
    let cg_point = CGPoint::new(cg_x, cg_y);

    debug_log(format!(
        "[monitor] cursor ns=({}, {}) cg=({}, {})",
        ns_point.x, ns_point.y, cg_x, cg_y
    ));

    // Find which display contains this CG point
    match CGDisplay::displays_with_point(cg_point, 16) {
        Ok((display_ids, count)) if count > 0 => {
            for &id in &display_ids[..count as usize] {
                let display = CGDisplay::new(id);
                if !display.is_active() {
                    continue;
                }
                // Skip mirrored displays — prefer the primary in a mirror set
                if display.is_in_mirror_set() && display.mirrors_display() != 0 {
                    continue;
                }

                let bounds = display.bounds();
                let logical_w = bounds.size.width;
                let pixels_w = display.pixels_wide();
                let scale_factor = if logical_w > 0.0 {
                    (pixels_w as f64) / logical_w
                } else {
                    1.0
                };

                debug_log(format!(
                    "[monitor] found cursor display id={} x={} y={} w={} h={} scale={}",
                    id,
                    bounds.origin.x,
                    bounds.origin.y,
                    bounds.size.width,
                    bounds.size.height,
                    scale_factor
                ));

                tracing::info!("[PERF][capture] find_cursor_monitor: {:?}", t0.elapsed());

                return Ok(MonitorInfo {
                    scale_factor,
                    x: bounds.origin.x as i32,
                    y: bounds.origin.y as i32,
                    width: logical_w as u32,
                    height: bounds.size.height as u32,
                    display_id: id,
                });
            }
        }
        _ => {}
    }

    // Fallback to main display
    debug_log("[monitor] no display found for cursor, falling back to main");
    find_primary_screen()
}

/// Find the primary monitor (fallback / self-test).
#[cfg(not(target_os = "macos"))]
pub fn find_primary_screen() -> AppResult<CursorMonitorResult> {
    let t0 = std::time::Instant::now();
    let screens = CaptureScreen::all().map_err(|e| AppError::Capture(e.to_string()))?;
    tracing::info!("[PERF][capture] Screen::all(): {:?}", t0.elapsed());

    let primary = screens
        .into_iter()
        .find(|s| s.display_info.is_primary)
        .ok_or_else(|| AppError::Capture("no primary monitor found".into()))?;

    let scale_factor = primary.display_info.scale_factor as f64;
    let x = primary.display_info.x;
    let y = primary.display_info.y;
    let width = primary.display_info.width;
    let height = primary.display_info.height;

    Ok(CursorMonitorResult {
        screen: primary,
        monitor: MonitorInfo {
            scale_factor,
            x,
            y,
            width,
            height,
        },
    })
}

/// Find the primary monitor (fallback / self-test).
#[cfg(target_os = "macos")]
pub fn find_primary_screen() -> AppResult<MonitorInfo> {
    use core_graphics::display::CGDisplay;

    let t0 = std::time::Instant::now();
    let main_display = CGDisplay::main();
    let bounds = main_display.bounds();

    let logical_w = bounds.size.width;
    let logical_h = bounds.size.height;
    let pixels_w = main_display.pixels_wide();

    let scale_factor = if logical_w > 0.0 {
        (pixels_w as f64) / logical_w
    } else {
        1.0
    };

    let x = bounds.origin.x as i32;
    let y = bounds.origin.y as i32;
    let width = logical_w as u32;
    let height = logical_h as u32;

    tracing::info!("[PERF][capture] CGDisplay::main: {:?}", t0.elapsed());

    debug_log(format!(
        "[monitor] primary x={} y={} width={} height={} scale_factor={}",
        x, y, width, height, scale_factor
    ));

    Ok(MonitorInfo {
        scale_factor,
        x,
        y,
        width,
        height,
        display_id: main_display.id,
    })
}

// ── Cursor position (non-macOS) ─────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn get_cursor_position() -> Result<(i32, i32), String> {
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    extern "system" {
        fn GetCursorPos(lpPoint: *mut Point) -> i32;
    }
    let mut pt = Point { x: 0, y: 0 };
    let result = unsafe { GetCursorPos(&mut pt) };
    if result == 0 {
        return Err("GetCursorPos failed".into());
    }
    Ok((pt.x, pt.y))
}

#[cfg(target_os = "linux")]
fn get_cursor_position() -> Result<(i32, i32), String> {
    // On Linux X11, use XQueryPointer to get cursor position.
    // Fall back to (0, 0) which will resolve to the primary display.
    // Wayland does not expose global cursor coordinates; from_point
    // will still find a valid display.
    #[cfg(feature = "x11")]
    {
        // Attempt X11 if available
        use std::ffi::c_void;
        use std::ptr;

        extern "C" {
            fn XOpenDisplay(name: *const c_void) -> *mut c_void;
            fn XCloseDisplay(display: *mut c_void);
            fn XDefaultRootWindow(display: *mut c_void) -> u64;
            fn XQueryPointer(
                display: *mut c_void,
                window: u64,
                root_return: *mut u64,
                child_return: *mut u64,
                root_x_return: *mut i32,
                root_y_return: *mut i32,
                win_x_return: *mut i32,
                win_y_return: *mut i32,
                mask_return: *mut u32,
            ) -> i32;
        }

        let display = unsafe { XOpenDisplay(ptr::null()) };
        if display.is_null() {
            return Err("XOpenDisplay failed".into());
        }
        let root = unsafe { XDefaultRootWindow(display) };
        let mut root_x = 0i32;
        let mut root_y = 0i32;
        unsafe {
            XQueryPointer(
                display,
                root,
                &mut 0u64,
                &mut 0u64,
                &mut root_x,
                &mut root_y,
                &mut 0i32,
                &mut 0i32,
                &mut 0u32,
            );
            XCloseDisplay(display);
        }
        return Ok((root_x, root_y));
    }

    #[allow(unreachable_code)]
    Err("Linux cursor position not available without x11".into())
}

#[cfg(target_os = "linux")]
pub fn is_wayland_session() -> bool {
    matches!(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        Some("wayland")
    ) || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

#[cfg(target_os = "linux")]
pub fn capture_wayland_selection() -> AppResult<Option<LinuxWaylandCapture>> {
    let launch_env = detect_hyprland_wayland_env().ok().flatten();
    let slurp_output =
        run_wayland_capture_command("slurp", &["-f", "%x,%y %wx%h"], launch_env.as_ref()).map_err(
            |e| {
                AppError::Capture(format!(
                    "failed to launch slurp; install it with `sudo pacman -S slurp`: {e}"
                ))
            },
        )?;

    if !slurp_output.status.success() {
        return if slurp_output.status.code() == Some(1) {
            Ok(None)
        } else {
            Err(AppError::Capture(format_wayland_capture_failure(
                "slurp",
                &slurp_output.stderr,
                launch_env.as_ref(),
            )))
        };
    }

    let geometry = String::from_utf8(slurp_output.stdout)
        .map_err(|e| AppError::Capture(format!("slurp returned invalid UTF-8: {e}")))?;
    let geometry = geometry.trim();
    if geometry.is_empty() {
        return Ok(None);
    }

    let (rect_x, rect_y, width, height) = parse_slurp_geometry(geometry)?;
    if width <= 4 || height <= 4 {
        return Ok(None);
    }

    let grim_output =
        run_wayland_capture_command("grim", &["-g", geometry, "-"], launch_env.as_ref()).map_err(
            |e| {
                AppError::Capture(format!(
                    "failed to launch grim; install it with `sudo pacman -S grim`: {e}"
                ))
            },
        )?;

    let png_bytes = if grim_output.status.success() {
        grim_output.stdout
    } else if is_missing_wlr_screencopy_error(&grim_output.stderr) {
        capture_spectacle_fullscreen_crop(rect_x, rect_y, width, height).map_err(
            |fallback_err| {
                AppError::Capture(format!(
                    "{}; KDE/Spectacle fallback failed: {fallback_err}",
                    format_wayland_capture_failure(
                        "grim",
                        &grim_output.stderr,
                        launch_env.as_ref()
                    )
                ))
            },
        )?
    } else {
        return Err(AppError::Capture(format_wayland_capture_failure(
            "grim",
            &grim_output.stderr,
            launch_env.as_ref(),
        )));
    };

    let reader = image::ImageReader::new(std::io::Cursor::new(&png_bytes))
        .with_guessed_format()
        .map_err(AppError::Io)?;
    let image = reader.decode()?.into_rgba8();
    let width = image.width();
    let height = image.height();

    Ok(Some(LinuxWaylandCapture {
        png_bytes,
        rgba_bytes: image.into_raw(),
        width,
        height,
        rect_x,
        rect_y,
    }))
}

#[cfg(target_os = "linux")]
fn is_missing_wlr_screencopy_error(stderr: &[u8]) -> bool {
    String::from_utf8_lossy(stderr)
        .to_ascii_lowercase()
        .contains("screen capture protocol")
}

#[cfg(target_os = "linux")]
fn capture_spectacle_fullscreen_crop(
    rect_x: i32,
    rect_y: i32,
    width: u32,
    height: u32,
) -> AppResult<Vec<u8>> {
    let capture_path = temp_linux_capture_path("png");
    let output = Command::new("spectacle")
        .args([
            "--background",
            "--nonotify",
            "--fullscreen",
            "--output",
            capture_path.to_string_lossy().as_ref(),
        ])
        .output()
        .map_err(|e| AppError::Capture(format!("failed to launch spectacle: {e}")))?;

    if !output.status.success() {
        let _ = fs::remove_file(&capture_path);
        return Err(AppError::Capture(format!(
            "spectacle failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    wait_for_nonempty_file(&capture_path, Duration::from_secs(5))?;
    let fullscreen_png = fs::read(&capture_path)?;
    let _ = fs::remove_file(&capture_path);
    crop_png_bytes(&fullscreen_png, rect_x, rect_y, width, height)
}

#[cfg(target_os = "linux")]
fn wait_for_nonempty_file(path: &PathBuf, timeout: Duration) -> AppResult<()> {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if fs::metadata(path)
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false)
        {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }

    Err(AppError::Capture(format!(
        "spectacle did not create screenshot file in time: {}",
        path.display()
    )))
}

#[cfg(target_os = "linux")]
fn crop_png_bytes(
    png_bytes: &[u8],
    rect_x: i32,
    rect_y: i32,
    width: u32,
    height: u32,
) -> AppResult<Vec<u8>> {
    let image = image::load_from_memory(png_bytes)
        .map_err(AppError::Image)?
        .into_rgba8();
    let image_width = image.width();
    let image_height = image.height();

    let x = rect_x.max(0) as u32;
    let y = rect_y.max(0) as u32;
    if x >= image_width || y >= image_height {
        return Err(AppError::Capture(format!(
            "selected region starts outside spectacle screenshot: region=({}, {}) {}x{}, screenshot={}x{}",
            rect_x, rect_y, width, height, image_width, image_height
        )));
    }

    let crop_width = width.min(image_width - x);
    let crop_height = height.min(image_height - y);
    if crop_width == 0 || crop_height == 0 {
        return Err(AppError::Capture(
            "selected region is empty after spectacle crop".into(),
        ));
    }

    let cropped = image::imageops::crop_imm(&image, x, y, crop_width, crop_height).to_image();
    encode_rgba_png(cropped)
}

#[cfg(target_os = "linux")]
fn temp_linux_capture_path(ext: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    env::temp_dir().join(format!(
        "glance-wayland-capture-{}-{ts}.{ext}",
        std::process::id()
    ))
}

#[cfg(target_os = "linux")]
fn encode_rgba_png(image: image::RgbaImage) -> AppResult<Vec<u8>> {
    let mut bytes = Vec::new();
    image
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .map_err(|e| AppError::Capture(format!("png encode failed: {e}")))?;
    Ok(bytes)
}

#[cfg(target_os = "linux")]
fn run_wayland_capture_command(
    program: &str,
    args: &[&str],
    launch_env: Option<&WaylandLaunchEnv>,
) -> std::io::Result<Output> {
    let mut command = Command::new(program);
    command.args(args);

    if let Some(launch_env) = launch_env {
        command.env("WAYLAND_DISPLAY", &launch_env.wayland_display);
        if let Some(signature) = launch_env.hyprland_instance_signature.as_deref() {
            command.env("HYPRLAND_INSTANCE_SIGNATURE", signature);
        }
        if let Some(runtime_dir) = launch_env.xdg_runtime_dir.as_deref() {
            command.env("XDG_RUNTIME_DIR", runtime_dir);
        }
    }

    command.output()
}

#[cfg(target_os = "linux")]
fn format_wayland_capture_failure(
    program: &str,
    stderr: &[u8],
    launch_env: Option<&WaylandLaunchEnv>,
) -> String {
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    let mut message = format!("{program} failed: {stderr}");

    if let Some(launch_env) = launch_env {
        if let Some(signature) = launch_env.hyprland_instance_signature.as_deref() {
            message.push_str(&format!(
                " (Hyprland env override: WAYLAND_DISPLAY={}, HYPRLAND_INSTANCE_SIGNATURE={})",
                launch_env.wayland_display, signature
            ));
        } else {
            message.push_str(&format!(
                " (Wayland env override: WAYLAND_DISPLAY={})",
                launch_env.wayland_display
            ));
        }
    }

    if stderr.contains("screen capture protocol") {
        message.push_str(
            "; if this was launched by a desktop entry or app-level global shortcut, prefer binding `glance --capture` directly in `hyprland.conf`",
        );
    }

    message
}

#[cfg(target_os = "linux")]
fn detect_hyprland_wayland_env() -> AppResult<Option<WaylandLaunchEnv>> {
    if !is_probably_hyprland_session() {
        return Ok(None);
    }

    let current_wayland = env::var("WAYLAND_DISPLAY").ok();
    let current_signature = env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
    let current_runtime_dir = env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);

    let instances = match query_hyprland_instances() {
        Ok(instances) => instances,
        Err(err) => {
            tracing::warn!("hyprland env recovery skipped: {err}");
            return Ok(None);
        }
    };
    let Some(selected) = select_hyprland_instance(
        &instances,
        current_signature.as_deref(),
        current_wayland.as_deref(),
    ) else {
        return Ok(None);
    };

    let launch_env = WaylandLaunchEnv {
        wayland_display: selected.wl_socket.clone(),
        hyprland_instance_signature: Some(selected.instance.clone()),
        xdg_runtime_dir: current_runtime_dir.clone(),
    };

    let needs_override = current_wayland.as_deref() != Some(selected.wl_socket.as_str())
        || current_signature.as_deref() != Some(selected.instance.as_str());

    if needs_override {
        Ok(Some(launch_env))
    } else {
        Ok(None)
    }
}

#[cfg(target_os = "linux")]
fn is_probably_hyprland_session() -> bool {
    env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok()
        || env::var("XDG_CURRENT_DESKTOP")
            .ok()
            .map(|value| value.to_ascii_lowercase().contains("hypr"))
            .unwrap_or(false)
        || env::var("DESKTOP_SESSION")
            .ok()
            .map(|value| value.to_ascii_lowercase().contains("hypr"))
            .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn query_hyprland_instances() -> AppResult<Vec<HyprlandInstanceRecord>> {
    let output = Command::new("hyprctl")
        .args(["-j", "instances"])
        .output()
        .map_err(|e| {
            AppError::Capture(format!(
                "failed to launch hyprctl while preparing Hyprland capture env: {e}"
            ))
        })?;

    if !output.status.success() {
        return Err(AppError::Capture(format!(
            "hyprctl -j instances failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let instances: Vec<HyprlandInstanceRecord> = serde_json::from_slice(&output.stdout)?;
    Ok(instances
        .into_iter()
        .filter(|instance| !instance.instance.is_empty() && !instance.wl_socket.is_empty())
        .collect())
}

#[cfg(target_os = "linux")]
fn select_hyprland_instance<'a>(
    instances: &'a [HyprlandInstanceRecord],
    current_signature: Option<&str>,
    current_wayland: Option<&str>,
) -> Option<&'a HyprlandInstanceRecord> {
    if let Some(signature) = current_signature {
        if let Some(instance) = instances
            .iter()
            .find(|instance| instance.instance == signature)
        {
            return Some(instance);
        }
    }

    if let Some(wayland_display) = current_wayland {
        if let Some(instance) = instances
            .iter()
            .find(|instance| instance.wl_socket == wayland_display)
        {
            return Some(instance);
        }
    }

    if instances.len() == 1 {
        return instances.first();
    }

    None
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use super::{select_hyprland_instance, HyprlandInstanceRecord};

    #[test]
    fn prefers_current_hyprland_signature() {
        let instances = vec![
            HyprlandInstanceRecord {
                instance: "sig-a".into(),
                wl_socket: "wayland-0".into(),
            },
            HyprlandInstanceRecord {
                instance: "sig-b".into(),
                wl_socket: "wayland-1".into(),
            },
        ];

        let selected =
            select_hyprland_instance(&instances, Some("sig-b"), Some("wayland-0")).unwrap();

        assert_eq!(selected.instance, "sig-b");
        assert_eq!(selected.wl_socket, "wayland-1");
    }

    #[test]
    fn falls_back_to_single_instance_when_env_is_missing() {
        let instances = vec![HyprlandInstanceRecord {
            instance: "sig-a".into(),
            wl_socket: "wayland-0".into(),
        }];

        let selected = select_hyprland_instance(&instances, None, None).unwrap();

        assert_eq!(selected.instance, "sig-a");
    }
}

#[cfg(target_os = "linux")]
fn parse_slurp_geometry(geometry: &str) -> AppResult<(i32, i32, u32, u32)> {
    let (position, size) = geometry
        .split_once(' ')
        .ok_or_else(|| AppError::Parse("slurp output missing size".into()))?;
    let (x_str, y_str) = position
        .split_once(',')
        .ok_or_else(|| AppError::Parse("slurp output missing coordinates".into()))?;
    let (width_str, height_str) = size
        .split_once('x')
        .ok_or_else(|| AppError::Parse("slurp output missing dimensions".into()))?;

    let x = x_str
        .parse::<i32>()
        .map_err(|e| AppError::Parse(format!("invalid slurp x: {e}")))?;
    let y = y_str
        .parse::<i32>()
        .map_err(|e| AppError::Parse(format!("invalid slurp y: {e}")))?;
    let width = width_str
        .parse::<u32>()
        .map_err(|e| AppError::Parse(format!("invalid slurp width: {e}")))?;
    let height = height_str
        .parse::<u32>()
        .map_err(|e| AppError::Parse(format!("invalid slurp height: {e}")))?;
    Ok((x, y, width, height))
}

// ── Screen capture ──────────────────────────────────────────────────────────

/// Capture the screen to raw RGBA bytes in memory (no file I/O).
#[cfg(not(target_os = "macos"))]
pub fn capture_screen_to_memory(screen: CaptureScreen) -> AppResult<(Vec<u8>, u32, u32)> {
    let t0 = std::time::Instant::now();
    let capture = screen
        .capture()
        .map_err(|e| AppError::Capture(e.to_string()))?;
    #[cfg(target_os = "windows")]
    tracing::info!(
        "[PERF][capture] screen.capture() (BitBlt): {:?}",
        t0.elapsed()
    );
    #[cfg(target_os = "macos")]
    tracing::info!(
        "[PERF][capture] screen.capture() (CoreGraphics): {:?}",
        t0.elapsed()
    );
    #[cfg(target_os = "linux")]
    tracing::info!("[PERF][capture] screen.capture(): {:?}", t0.elapsed());

    let w = capture.width();
    let h = capture.height();
    let rgba_bytes = capture.into_raw();
    tracing::info!(
        "[PERF][capture] raw RGBA bytes: {} ({:.1} MB), {}x{}",
        rgba_bytes.len(),
        rgba_bytes.len() as f64 / 1_048_576.0,
        w,
        h
    );

    Ok((rgba_bytes, w, h))
}

#[cfg(target_os = "macos")]
pub fn capture_screen_to_memory(display_id: u32) -> AppResult<(Vec<u8>, u32, u32)> {
    let captured = capture_screen_with_preview(display_id)?;
    Ok((captured.rgba_bytes, captured.width, captured.height))
}

#[cfg(target_os = "macos")]
pub fn capture_screen_with_preview(display_id: u32) -> AppResult<CapturedScreenImage> {
    capture_screen_with_preview_macos(display_id)
}

#[cfg(target_os = "macos")]
pub fn capture_interactive_region() -> AppResult<Option<InteractiveCaptureImage>> {
    capture_interactive_region_macos()
}

#[cfg(target_os = "macos")]
fn capture_screen_with_preview_macos(display_id: u32) -> AppResult<CapturedScreenImage> {
    let started = std::time::Instant::now();
    let capture_path = temp_capture_path("jpg");

    let display_id_str = display_id.to_string();
    let status = Command::new("screencapture")
        .args([
            "-x",
            "-D",
            &display_id_str,
            "-t",
            "jpg",
            capture_path.to_string_lossy().as_ref(),
        ])
        .status()
        .map_err(|e| AppError::Capture(format!("failed to run screencapture: {e}")))?;

    if !status.success() {
        return Err(AppError::Capture(format!(
            "screencapture exited with status {status}"
        )));
    }

    let jpeg_bytes = fs::read(&capture_path)?;
    let _ = fs::remove_file(&capture_path);
    tracing::info!("[PERF][capture] screencapture jpg: {:?}", started.elapsed());

    let decode_started = std::time::Instant::now();
    let image = image::load_from_memory(&jpeg_bytes)
        .map_err(AppError::Image)?
        .into_rgba8();
    tracing::info!(
        "[PERF][capture] decode screenshot jpg: {:?}",
        decode_started.elapsed()
    );

    let w = image.width();
    let h = image.height();
    if should_write_debug_capture_images() {
        debug_write_bytes("01_screencapture.jpg", &jpeg_bytes);
        if let Ok(png_bytes) = encode_rgba_png(image.clone()) {
            debug_write_bytes("01_screencapture.png", &png_bytes);
        }
    }
    let rgba_bytes = image.into_raw();
    debug_log(format!(
        "[capture] screencapture -> {}x{} rgba_bytes={}",
        w,
        h,
        rgba_bytes.len()
    ));
    tracing::info!(
        "[PERF][capture] raw RGBA bytes: {} ({:.1} MB), {}x{}",
        rgba_bytes.len(),
        rgba_bytes.len() as f64 / 1_048_576.0,
        w,
        h
    );

    Ok(CapturedScreenImage {
        preview_bytes: jpeg_bytes,
        preview_mime: "image/jpeg",
        rgba_bytes,
        width: w,
        height: h,
    })
}

#[cfg(target_os = "macos")]
fn capture_interactive_region_macos() -> AppResult<Option<InteractiveCaptureImage>> {
    let started = std::time::Instant::now();
    let capture_path = temp_capture_path("png");

    let output = Command::new("screencapture")
        .args([
            "-i",
            "-x",
            "-t",
            "png",
            capture_path.to_string_lossy().as_ref(),
        ])
        .output()
        .map_err(|e| AppError::Capture(format!("failed to run interactive screencapture: {e}")))?;

    let capture_exists = capture_path.exists();
    if !output.status.success() && !capture_exists {
        debug_log(format!(
            "[capture] interactive screencapture cancelled status={:?}",
            output.status.code()
        ));
        return Ok(None);
    }

    if !capture_exists {
        debug_log("[capture] interactive screencapture finished without output file");
        return Ok(None);
    }

    let png_bytes = fs::read(&capture_path)?;
    let _ = fs::remove_file(&capture_path);

    if png_bytes.is_empty() {
        debug_log("[capture] interactive screencapture produced empty file");
        return Ok(None);
    }

    tracing::info!(
        "[PERF][capture] screencapture interactive png: {:?}",
        started.elapsed()
    );

    let decode_started = std::time::Instant::now();
    let image = image::load_from_memory_with_format(&png_bytes, image::ImageFormat::Png)
        .map_err(AppError::Image)?
        .into_rgba8();
    tracing::info!(
        "[PERF][capture] decode interactive screenshot png: {:?}",
        decode_started.elapsed()
    );

    let width = image.width();
    let height = image.height();
    let rgba_bytes = image.into_raw();

    debug_log(format!(
        "[capture] interactive screencapture -> {}x{} rgba_bytes={} status={:?}",
        width,
        height,
        rgba_bytes.len(),
        output.status.code()
    ));

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug_log(format!(
            "[capture] interactive screencapture returned non-zero but wrote output: {}",
            stderr.trim()
        ));
    }

    Ok(Some(InteractiveCaptureImage {
        png_bytes,
        rgba_bytes,
        width,
        height,
    }))
}

// ── Debug helpers (macOS) ─────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub fn debug_reset_dir() -> AppResult<PathBuf> {
    let dir = PathBuf::from(DEBUG_CAPTURE_DIR);
    if dir.exists() {
        let _ = fs::remove_dir_all(&dir);
    }
    fs::create_dir_all(&dir).map_err(|e| {
        AppError::Capture(format!(
            "failed to create debug dir {DEBUG_CAPTURE_DIR}: {e}"
        ))
    })?;
    Ok(dir)
}

#[cfg(target_os = "macos")]
pub fn debug_dir() -> PathBuf {
    PathBuf::from(DEBUG_CAPTURE_DIR)
}

#[cfg(target_os = "macos")]
pub fn debug_log(message: impl AsRef<str>) {
    let dir = PathBuf::from(DEBUG_CAPTURE_DIR);
    let _ = fs::create_dir_all(&dir);
    let log_path = dir.join("capture.log");
    let line = format!("{}\n", message.as_ref());
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
    {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(target_os = "macos")]
pub fn debug_write_bytes(file_name: &str, bytes: &[u8]) {
    let dir = PathBuf::from(DEBUG_CAPTURE_DIR);
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(dir.join(file_name), bytes);
}

#[cfg(target_os = "macos")]
fn should_write_debug_capture_images() -> bool {
    std::env::var_os("GLANCE_CAPTURE_DEBUG_IMAGES").is_some()
}

#[cfg(target_os = "macos")]
fn temp_capture_path(ext: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    PathBuf::from(format!("/private/tmp/glance-capture-{ts}.{ext}"))
}

#[cfg(target_os = "macos")]
fn encode_rgba_png(image: image::RgbaImage) -> AppResult<Vec<u8>> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .map_err(|e| AppError::Capture(format!("debug png encode failed: {e}")))?;
    Ok(bytes)
}
