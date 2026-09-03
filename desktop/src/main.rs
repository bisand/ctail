//! ctail desktop — the Linux/Windows front end (runs on macOS too, for
//! development). One window drawn by DeniseUI's software rasteriser, the log
//! engine from `ctail-core` underneath, no webview and no GPU requirement.

mod app;
mod fonts;
mod logview;
mod profiles;
mod prompt;
mod search;
mod settings;
mod theme;
mod widgets;

use denise::Size;
use denise_winit::{run_with, WindowConfig};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some("--snapshot") {
        let what = args.next().unwrap_or_else(|| "settings".into());
        let path = args.next().unwrap_or_else(|| format!("{what}.ppm"));
        let scale: f32 = args.next().and_then(|a| a.parse().ok()).unwrap_or(2.0);
        return snapshot(&what, &path, scale).map_err(Into::into);
    }
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

/// Paints one of the app's windows into a buffer and writes it as a PPM, so a
/// layout can be checked without a display — the same affordance Denise's own
/// examples carry, and the only way to see one on a machine whose screen has
/// gone to sleep. `what` is "settings" or "profiles".
fn snapshot(what: &str, path: &str, scale: f32) -> std::io::Result<()> {
    use denise::{BufferAge, PixelFormat};
    use denise_winit::DeniseApp;
    use std::io::Write as _;

    let logical = match what {
        "profiles" => profiles::SIZE,
        _ => settings::SIZE,
    };
    let size = Size::new(
        (logical.width as f32 * scale + 0.5) as u32,
        (logical.height as f32 * scale + 0.5) as u32,
    );
    let (tx, _rx) = std::sync::mpsc::channel();
    let (ptx, _prx) = std::sync::mpsc::channel();
    let mut window: Box<dyn DeniseApp> = match what {
        "profiles" => Box::new(profiles::ProfilesWindow::new(size, scale, ptx)),
        _ => Box::new(settings::SettingsWindow::new(size, scale, tx)),
    };
    let mut pixels = vec![0u32; (size.width * size.height) as usize];
    {
        let mut frame = denise::Frame::new(
            &mut pixels,
            size,
            size.width,
            PixelFormat::Xrgb8888,
            BufferAge::Undefined,
        )
        .expect("frame");
        window.render(&mut frame, &[]);
    }
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    write!(out, "P6\n{} {}\n255\n", size.width, size.height)?;
    for word in &pixels {
        out.write_all(&[(word >> 16) as u8, (word >> 8) as u8, *word as u8])?;
    }
    out.flush()?;
    eprintln!("wrote {path} at {}x{}", size.width, size.height);
    Ok(())
}
