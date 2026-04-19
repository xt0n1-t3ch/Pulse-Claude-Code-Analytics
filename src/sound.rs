//! Sound notifications using the Windows Beep API (no extra dependencies).
//! On non-Windows platforms all functions are silent no-ops.

/// Play a pleasant C5-E5-G5-C6 ascending arpeggio to signal an Extra Usage change.
pub fn play_extra_usage_alert() {
    // Notes: (frequency_hz, duration_ms)
    // C5=523, E5=659, G5=784, C6=1047
    let notes: &[(u32, u32)] = &[(523, 90), (659, 90), (784, 90), (1047, 200)];

    #[cfg(windows)]
    {
        unsafe extern "system" {
            fn Beep(dwFreq: u32, dwDuration: u32) -> i32;
        }
        for &(freq, dur) in notes {
            unsafe {
                Beep(freq, dur);
            }
        }
    }

    // Suppress unused-variable warning on non-Windows
    #[cfg(not(windows))]
    let _ = notes;
}
