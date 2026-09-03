//! Finds a monospace face for the log and a UI face for the chrome on the
//! machine we run on, falling back to Denise's built-in bitmap font.

use denise_text::{GlyphSource, TrueTypeSource};
use std::path::{Path, PathBuf};

const DIRS: &[&str] = &[
    "/usr/share/fonts",
    "/usr/local/share/fonts",
    "/System/Library/Fonts",
    "/System/Library/Fonts/Supplemental",
    "/Library/Fonts",
    "C:\\Windows\\Fonts",
];

pub const MONO: &[&str] = &[
    "SFNSMono.ttf",
    "DejaVuSansMono.ttf",
    "LiberationMono-Regular.ttf",
    "JetBrainsMono-Regular.ttf",
    "consola.ttf",
    "Andale Mono.ttf",
    "cour.ttf",
];

pub const UI: &[&str] = &[
    "SFNS.ttf",
    "DejaVuSans.ttf",
    "LiberationSans-Regular.ttf",
    "NotoSans-Regular.ttf",
    "segoeui.ttf",
    "Arial.ttf",
];

pub fn load(preferred: &[&str]) -> Option<(String, Box<dyn GlyphSource>)> {
    let mut found: Vec<PathBuf> = Vec::new();
    for dir in DIRS {
        collect(Path::new(dir), 0, &mut found);
    }
    let path = preferred.iter().find_map(|want| {
        found
            .iter()
            .find(|p| p.file_name().is_some_and(|n| n.eq_ignore_ascii_case(want)))
    })?;
    let name = path.display().to_string();
    let bytes = std::fs::read(path).ok()?;
    let source = TrueTypeSource::from_bytes(&name, &bytes).ok()?;
    Some((name, Box::new(source)))
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > 3 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, depth + 1, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ttf"))
        {
            out.push(path);
        }
    }
}
