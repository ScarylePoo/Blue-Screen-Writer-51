//! Small OS-specific helpers (disk space, hidden files, time zone).

use std::path::Path;

#[cfg(windows)]
mod win {
    #[repr(C)]
    pub struct SystemTime(pub [u16; 8]);
    #[repr(C)]
    pub struct Tzi {
        pub bias: i32,
        pub std_name: [u16; 32],
        pub std_date: SystemTime,
        pub std_bias: i32,
        pub dl_name: [u16; 32],
        pub dl_date: SystemTime,
        pub dl_bias: i32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GetDiskFreeSpaceExW(dir: *const u16, avail: *mut u64, total: *mut u64, free: *mut u64) -> i32;
        pub fn GetTimeZoneInformation(tz: *mut Tzi) -> u32;
        pub fn FileTimeToSystemTime(ft: *const u64, st: *mut SystemTime) -> i32;
        pub fn SystemTimeToTzSpecificLocalTime(tz: *const Tzi, utc: *const SystemTime, local: *mut SystemTime) -> i32;
        pub fn SystemTimeToFileTime(st: *const SystemTime, ft: *mut u64) -> i32;
    }
}

/// (free bytes available to the user, total bytes)
pub fn disk_space(p: &Path) -> Option<(u64, u64)> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let mut wide: Vec<u16> = p.as_os_str().encode_wide().collect();
        if !wide.ends_with(&[b'\\' as u16]) {
            wide.push(b'\\' as u16);
        }
        wide.push(0);
        let (mut a, mut t, mut f) = (0u64, 0u64, 0u64);
        let ok = unsafe { win::GetDiskFreeSpaceExW(wide.as_ptr(), &mut a, &mut t, &mut f) };
        if ok != 0 { Some((a, t)) } else { None }
    }
    #[cfg(not(windows))]
    {
        let _ = p;
        None
    }
}

pub fn is_hidden(de: &std::fs::DirEntry) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if let Ok(m) = de.metadata() {
            // HIDDEN | SYSTEM
            return m.file_attributes() & 0x6 != 0;
        }
        false
    }
    #[cfg(not(windows))]
    {
        de.file_name().to_string_lossy().starts_with('.')
    }
}

/// Seconds to add to UTC to get local time.
pub fn utc_offset_secs() -> i64 {
    #[cfg(windows)]
    {
        let mut tz: win::Tzi = unsafe { std::mem::zeroed() };
        let r = unsafe { win::GetTimeZoneInformation(&mut tz) };
        let extra = if r == 2 { tz.dl_bias } else { tz.std_bias };
        -((tz.bias + extra) as i64) * 60
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// Seconds to add to the UTC time `unix_secs` to get the local time *at that
/// moment* — summer/winter time of that date, not of today.
pub fn utc_offset_at(unix_secs: i64) -> i64 {
    #[cfg(windows)]
    {
        const EPOCH_DIFF: i64 = 11_644_473_600; // 1601 -> 1970
        let ft = ((unix_secs + EPOCH_DIFF).max(0) as u64) * 10_000_000;
        let mut utc = win::SystemTime([0; 8]);
        let mut local = win::SystemTime([0; 8]);
        let mut lft = 0u64;
        let ok = unsafe {
            win::FileTimeToSystemTime(&ft, &mut utc) != 0
                && win::SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut local) != 0
                && win::SystemTimeToFileTime(&local, &mut lft) != 0
        };
        if ok { (lft as i64 - ft as i64) / 10_000_000 } else { utc_offset_secs() }
    }
    #[cfg(not(windows))]
    {
        let _ = unix_secs;
        0
    }
}

/// Default paper (width, height) in WPU from the Windows locale.
pub fn default_paper() -> (i32, i32) {
    static P: std::sync::OnceLock<(i32, i32)> = std::sync::OnceLock::new();
    *P.get_or_init(|| {
        #[cfg(windows)]
        {
            #[link(name = "kernel32")]
            unsafe extern "system" {
                fn GetLocaleInfoEx(name: *const u16, lctype: u32, data: *mut u16, n: i32) -> i32;
            }
            let mut v = [0u16; 4];
            // LOCALE_IPAPERSIZE | LOCALE_RETURN_NUMBER
            let r = unsafe { GetLocaleInfoEx(std::ptr::null(), 0x100A | 0x2000_0000, v.as_mut_ptr(), 2) };
            if r > 0 {
                let code = v[0] as u32 | ((v[1] as u32) << 16);
                if code == 1 {
                    return (10200, 13200); // Letter
                }
                if code == 5 {
                    return (10200, 16800); // Legal
                }
                return (9921, 14031); // A4
            }
        }
        (9921, 14031)
    })
}

/// Local date/time now: (year, month, day, hour, minute, weekday 0=Sunday)
pub fn now_local() -> (i64, u32, u32, u32, u32, u32) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        + utc_offset_secs();
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (y, m, d) = crate::filelist::civil_from_days(days);
    let wd = ((days + 4).rem_euclid(7)) as u32; // 1970-01-01 was Thursday
    (y, m, d, (rem / 3600) as u32, ((rem % 3600) / 60) as u32, wd)
}
