use screenshots::Screen;

use crate::error::{AppError, AppResult};

/// Monitor info returned by find_primary_screen.
pub struct PrimaryMonitorInfo {
    pub screen: Screen,
    pub scale_factor: f64,
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

    let scale_factor = primary.display_info.scale_factor;

    Ok(PrimaryMonitorInfo {
        screen: primary,
        scale_factor,
    })
}

/// Capture the screen to raw RGBA bytes in memory (no file I/O).
pub fn capture_screen_to_memory(screen: Screen) -> AppResult<(Vec<u8>, u32, u32)> {
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

