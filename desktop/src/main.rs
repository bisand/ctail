//! ctail desktop — the Linux/Windows front end (runs on macOS too, for
//! development). One window drawn by DeniseUI's software rasteriser, the log
//! engine from `ctail-core` underneath, no webview and no GPU requirement.

mod app;
mod fonts;
mod logview;
mod theme;

use denise::Size;
use denise_winit::{run_with, WindowConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let files: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .filter_map(|a| std::fs::canonicalize(&a).ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let settings = ctail_core::ConfigStore::new(None).load_settings();
    let (w, h) = if settings.window.width > 0 && settings.window.height > 0 {
        (settings.window.width as u32, settings.window.height as u32)
    } else {
        (1200, 800)
    };
    run_with(
        WindowConfig {
            title: "ctail".into(),
            size: Size::new(w, h),
            ..WindowConfig::default()
        },
        move |size, scale| app::App::new(size, scale, files.clone()),
    )?;
    Ok(())
}
