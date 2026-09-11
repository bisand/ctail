//! ctail desktop — the Linux/Windows front end (runs on macOS too, for
//! development). One window drawn by DeniseUI through the GPU, or by its
//! software rasteriser where there is no GPU to draw with, the log engine from
//! `ctail-core` underneath, and no webview.

mod app;
mod assistant;
mod fonts;
mod logview;
mod memory;
mod profiles;
mod prompt;
mod search;
mod settings;
mod statusbar;
mod tabbar;
mod theme;
mod trace;
mod widgets;

use denise::Size;
use denise_winit::{run_with, Error, Present, WindowConfig};

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
    let config = |present| WindowConfig {
        title: "ctail".into(),
        size: Size::new(w, h),
        present,
        ..WindowConfig::default()
    };
    // The GPU first, because it is what paces frames to the display: a
    // swapchain presents one frame per refresh, where the software path
    // presents whenever an event has been handled, and an unpaced scroll is
    // what reads as choppy. The software rasteriser is the fallback for a
    // machine with no adapter that can present — a VM, a remote desktop, a
    // board without a driver — and an override, so the two can be compared.
    let present = match std::env::var("CTAIL_PRESENT").as_deref() {
        Ok("software") => Present::Software,
        _ => Present::Gpu,
    };
    let open = move |size, scale| app::App::new(size, scale, files, present);
    match run_with(config(present), open) {
        Err(Error::Gpu(reason)) if present == Present::Gpu => {
            eprintln!("ctail: cannot draw through the GPU ({reason}); drawing in software");
            run_again_in_software()
        }
        outcome => outcome.map_err(Into::into),
    }
}

/// Starts over with the software rasteriser chosen.
///
/// A process gets one event loop — winit refuses to make a second — so the
/// fallback is not a second `run_with` but a second process: this executable,
/// with the same arguments, told what to draw with. On Unix it replaces this
/// process; elsewhere it is waited for and its exit status handed on.
fn run_again_in_software() -> Result<(), Box<dyn std::error::Error>> {
    let mut again = std::process::Command::new(std::env::current_exe()?);
    again
        .args(std::env::args_os().skip(1))
        .env("CTAIL_PRESENT", "software");
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        // `exec` only returns when it failed.
        Err(again.exec().into())
    }
    #[cfg(not(unix))]
    {
        let status = again.status()?;
        std::process::exit(status.code().unwrap_or(1));
    }
}

/// Paints one of the app's windows into a buffer and writes it as a PPM, so a
/// layout can be checked without a display — the same affordance Denise's own
/// examples carry, and the only way to see one on a machine whose screen has
/// gone to sleep. `what` is "settings" or "profiles".
fn snapshot(what: &str, path: &str, scale: f32) -> std::io::Result<()> {
    use denise::{BufferAge, PixelFormat};
    use denise_winit::DeniseApp;
    use std::io::Write as _;

    if what == "main" {
        return snapshot_main(path, scale);
    }
    let logical = match what {
        "profiles" => profiles::SIZE,
        "assistant" => assistant::SIZE,
        _ => settings::SIZE,
    };
    let size = Size::new(
        (logical.width as f32 * scale + 0.5) as u32,
        (logical.height as f32 * scale + 0.5) as u32,
    );
    let (tx, _rx) = std::sync::mpsc::channel();
    let (ptx, _prx) = std::sync::mpsc::channel();
    let (atx, _arx) = std::sync::mpsc::channel();
    let mut window: Box<dyn DeniseApp> = match what {
        "profiles" => Box::new(profiles::ProfilesWindow::new(
            size,
            scale,
            ptx,
            Present::Software,
        )),
        "assistant" => {
            let mut w = assistant::AssistantWindow::new(size, scale, String::new(), atx);
            if let Ok(answer) = std::env::var("CTAIL_DEBUG_ANSWER") {
                w.debug_set_answer(&answer);
            }
            Box::new(w)
        }
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

/// Paints the main window, with whatever `CTAIL_DEBUG_*` asked to be open, so
/// the log surface and the menus can be checked without a display. The engine
/// runs on its own threads, so this pumps the application until lines arrive
/// rather than painting an empty window.
fn snapshot_main(path: &str, scale: f32) -> std::io::Result<()> {
    use denise::{BufferAge, DamageTracker, PixelFormat};
    use denise_winit::DeniseApp;
    use std::io::Write as _;

    let size = Size::new((1200.0 * scale + 0.5) as u32, (760.0 * scale + 0.5) as u32);
    let files: Vec<String> = std::env::var("CTAIL_DEBUG_FILE").into_iter().collect();
    let mut app = app::App::new(size, scale, files, Present::Software);
    // A picture is not a session: the debug file must not replace the tabs
    // the user has saved.
    app.without_saving_session();
    let mut damage = DamageTracker::new(size);
    let mut pixels = vec![0u32; (size.width * size.height) as usize];
    let paint = |app: &mut app::App, pixels: &mut Vec<u32>| {
        let mut frame = denise::Frame::new(
            pixels,
            size,
            size.width,
            PixelFormat::Xrgb8888,
            BufferAge::Undefined,
        )
        .expect("frame");
        app.render(&mut frame, &[]);
    };
    for _ in 0..40 {
        app.update(&[], &mut damage);
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    // Paint before touching anything: a view that has never been drawn does
    // not yet know how many rows it holds, and scrolling to a match asks it.
    paint(&mut app, &mut pixels);
    // A sideways offset clamps against what that first paint measured, so it
    // takes one more update to land.
    if std::env::var_os("CTAIL_DEBUG_SCROLL_X").is_some() {
        app.update(&[], &mut damage);
    }
    if let Ok(n) = std::env::var("CTAIL_DEBUG_SEARCH_STEP") {
        let forward = !n.starts_with('-');
        let times = n.trim_start_matches('-').parse().unwrap_or(1);
        app.debug_step_search(times, forward);
        // The jump is answered by the engine's thread, so let it land.
        for _ in 0..20 {
            app.update(&[], &mut damage);
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
    }
    if let Ok(which) = std::env::var("CTAIL_DEBUG_MENU") {
        match which.parse::<usize>() {
            Ok(i) => app.open_menu(i),
            Err(_) => app.open_tab_menu(0),
        }
        app.update(&[], &mut damage);
        // `CTAIL_DEBUG_MENU_HOVER="0,4;1,1"` then rests the pointer on those
        // rows in turn — panel, row — so a submenu opens for the picture.
        if let Ok(hovers) = std::env::var("CTAIL_DEBUG_MENU_HOVER") {
            for hover in hovers.split(';') {
                let mut parts = hover
                    .split(',')
                    .filter_map(|n| n.trim().parse::<usize>().ok());
                if let (Some(panel), Some(row)) = (parts.next(), parts.next()) {
                    app.debug_hover_menu(panel, row);
                    app.update(&[], &mut damage);
                }
            }
        }
    }
    paint(&mut app, &mut pixels);
    // A run of scrolling frames, each painted *incrementally* into the same
    // buffer — the rows moved, the strip drawn — and then the same state
    // painted whole into another. The two must not differ by a pixel: that
    // is the log view's claim that its scroll moved exactly what it said.
    if std::env::var_os("CTAIL_DEBUG_SCROLL_Y").is_some() {
        let steps: usize = std::env::var("CTAIL_DEBUG_SCROLL_STEPS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(10);
        for _ in 0..steps {
            app.update(&[], &mut damage);
            let mut frame = denise::Frame::new(
                &mut pixels,
                size,
                size.width,
                PixelFormat::Xrgb8888,
                BufferAge::Frames(1),
            )
            .expect("frame");
            app.render(&mut frame, &[]);
        }
        let mut whole = vec![0u32; pixels.len()];
        let mut frame = denise::Frame::new(
            &mut whole,
            size,
            size.width,
            PixelFormat::Xrgb8888,
            BufferAge::Undefined,
        )
        .expect("frame");
        app.render(&mut frame, &[]);
        drop(frame);
        let differing = pixels
            .iter()
            .zip(&whole)
            .filter(|(a, b)| (**a ^ **b) & 0x00FF_FFFF != 0)
            .count();
        eprintln!("after {steps} scrolled frames: {differing} pixels differ from a whole repaint");
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
