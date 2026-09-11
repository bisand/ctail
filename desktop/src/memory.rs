//! Byte counts for the status bar, and this process's memory footprint.
//!
//! The status bar shows what the open files hold — see `App::tick_memory` —
//! because a log viewer is a program people watch the memory of, and a window
//! over a 10 GB file is only honest if it can be seen not to be holding the
//! file. The whole process is a poor stand-in for that on the GPU path: the
//! graphics driver keeps a cache of its own while frames are being drawn,
//! 160 MB of it on a Mac, and gives it back when drawing stops.
//!
//! The footprint is still here, for the debug hooks that measure the process
//! itself. Each platform has its own name for "what this process is actually
//! costing", so each gets its own reading rather than a lowest common
//! denominator.

/// Bytes this process occupies, or `None` where the platform has no answer.
#[cfg(target_os = "macos")]
pub fn footprint() -> Option<u64> {
    // `phys_footprint` from the process's rusage: the same number Activity
    // Monitor shows, and the one the Swift front end reports.
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v2>::zeroed();
    let ok = unsafe {
        libc::proc_pid_rusage(
            std::process::id() as i32,
            libc::RUSAGE_INFO_V2,
            info.as_mut_ptr().cast(),
        )
    };
    (ok == 0).then(|| unsafe { info.assume_init() }.ri_phys_footprint)
}

/// Bytes this process occupies, or `None` where the platform has no answer.
#[cfg(target_os = "linux")]
pub fn footprint() -> Option<u64> {
    // VmRSS from /proc, in kB. Read rather than computed from statm because
    // the label is unambiguous and the file is a few hundred bytes.
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb * 1024)
}

/// Bytes this process occupies, or `None` where the platform has no answer.
#[cfg(windows)]
pub fn footprint() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    let mut counters = PROCESS_MEMORY_COUNTERS {
        cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        )
    };
    (ok != 0).then_some(counters.WorkingSetSize as u64)
}

/// Bytes this process occupies, or `None` where the platform has no answer.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn footprint() -> Option<u64> {
    None
}

/// The status bar's rendering of a byte count: kilobytes under a megabyte —
/// a few small logs hold less than that — whole megabytes until it takes four
/// digits to say, then gigabytes to two decimals.
pub fn format(bytes: u64) -> String {
    let kb = bytes as f64 / 1024.0;
    let mb = kb / 1024.0;
    if mb >= 1024.0 {
        format!("{:.2} GB", mb / 1024.0)
    } else if mb >= 1.0 {
        format!("{mb:.0} MB")
    } else {
        format!("{kb:.0} KB")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kilobytes_then_megabytes_until_it_takes_four_digits() {
        assert_eq!(format(0), "0 KB");
        assert_eq!(format(512 * 1024), "512 KB");
        assert_eq!(format(128 * 1024 * 1024), "128 MB");
        assert_eq!(format(1023 * 1024 * 1024), "1023 MB");
        assert_eq!(format(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format(3 * 1024 * 1024 * 1024 / 2), "1.50 GB");
    }

    #[test]
    fn the_process_can_read_its_own_footprint() {
        // Every platform this ships on answers; the fallback exists for the
        // ones it does not.
        if cfg!(any(target_os = "macos", target_os = "linux", windows)) {
            assert!(footprint().is_some_and(|b| b > 0));
        }
    }
}
