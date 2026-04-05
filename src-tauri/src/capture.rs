use screenshots::Screen;
#[cfg(target_os = "macos")]
use std::{fs, process::Command};

use crate::error::{AppError, AppResult};

/// Monitor info returned by find_primary_screen.
pub struct PrimaryMonitorInfo {
    pub screen: Screen,
    pub scale_factor: f64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Find the primary monitor (fast, ~5ms). Returns the Screen handle and display info.
pub fn find_primary_screen() -> AppResult<PrimaryMonitorInfo> {
    let t0 = std::time::Instant::now();
    let screens = Screen::all().map_err(|e| AppError::Capture(e.to_string()))?;
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

    Ok(PrimaryMonitorInfo {
        screen: primary,
        scale_factor,
        x,
        y,
        width,
        height,
    })
}

/// Capture the screen to raw RGBA bytes in memory (no file I/O).
pub fn capture_screen_to_memory(screen: Screen) -> AppResult<(Vec<u8>, u32, u32)> {
    #[cfg(target_os = "macos")]
    {
        return capture_screen_to_memory_macos(screen);
    }

    let t0 = std::time::Instant::now();
    let capture = screen
        .capture()
        .map_err(|e| AppError::Capture(e.to_string()))?;
    #[cfg(target_os = "windows")]
    tracing::info!("[PERF][capture] screen.capture() (BitBlt): {:?}", t0.elapsed());
    #[cfg(target_os = "macos")]
    tracing::info!("[PERF][capture] screen.capture() (CoreGraphics): {:?}", t0.elapsed());
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
fn capture_screen_to_memory_macos(_screen: Screen) -> AppResult<(Vec<u8>, u32, u32)> {
    let t0 = std::time::Instant::now();
    let output_path = std::env::temp_dir().join(format!("glance-capture-{}.png", uuid::Uuid::new_v4()));

    let output = Command::new("screencapture")
        .arg("-x")
        .arg("-m")
        .arg("-t")
        .arg("png")
        .arg(&output_path)
        .output()
        .map_err(|e| AppError::Capture(format!("failed to run screencapture: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::Capture(format!(
            "screencapture exited with status {}{}",
            output.status,
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        )));
    }

    let png_bytes = fs::read(&output_path)
        .map_err(|e| AppError::Capture(format!("failed to read screencapture output: {e}")))?;
    let _ = fs::remove_file(&output_path);

    let image = image::load_from_memory(&png_bytes)
        .map_err(|e| AppError::Capture(format!("failed to decode screencapture output: {e}")))?
        .into_rgba8();

    tracing::info!("[PERF][capture] screencapture -m: {:?}", t0.elapsed());
    let w = image.width();
    let h = image.height();
    let rgba_bytes = image.into_raw();
    tracing::info!(
        "[PERF][capture] raw RGBA bytes: {} ({:.1} MB), {}x{}",
        rgba_bytes.len(),
        rgba_bytes.len() as f64 / 1_048_576.0,
        w,
        h
    );

    Ok((rgba_bytes, w, h))
}
