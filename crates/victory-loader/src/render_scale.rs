//! `VICTORY_RENDER_SCALE=<mode>`: the title's internal render size.
//!
//! Grand Theft Auto VI (Series 1.0.2.1) renders at a fraction of its output
//! size and upscales: the function at RVA `0x1de4030` sets the internal width
//! and height (RVA `0x99fecf8`, `0x99fecfc`) to the output size (3840x2160 on
//! a Series X, whatever display mode we report) times a ratio it looks up by
//! a mode number (RVA `0x85642b4`) in two tables of fifteen: mode 1 is 1/2,
//! 2 is 2/3 (what the title uses: 2560x1440), 3 is 3/4 ... 10 is 29/30, 11 to
//! 15 are above 1. The mode's only writer is a setter at RVA `0x5eb77a0`
//! (`mov [mode], ecx; ret`), reached through function pointers. With the
//! switch set, that setter stores the asked mode instead of its argument, so
//! everything that reads the mode (the size function and four tests that it
//! is not 0) sees the same value from the start.
//!
//! The setter is patched only if its bytes are the ones of that build;
//! anything else is left alone (logged). Ratios below 1/2 do not exist in
//! the title's table, so 1 (1920x1080 inside a 3840x2160 output) is the
//! least work this offers.

/// RVA of the setter and its original bytes: `mov [rip+0x26acb0e], ecx;
/// ret` (the mode at RVA `0x85642b4`).
const SETTER_RVA: usize = 0x5eb_77a0;
const SETTER: [u8; 7] = [0x89, 0x0d, 0x0e, 0xcb, 0x6a, 0x02, 0xc3];
/// The mode's RVA, and the padding (`int3`) after the setter that the longer
/// replacement may use.
const MODE_RVA: usize = 0x856_42b4;
const PADDING: usize = 9;

/// The mode asked for, if the switch is set to one the title has (1 to 15).
fn asked() -> Option<u32> {
    let v = std::env::var("VICTORY_RENDER_SCALE").ok()?;
    let mode = v.trim().parse::<u32>().ok()?;
    if (1..=15).contains(&mode) {
        Some(mode)
    } else {
        tracing::warn!(value = %v, "VICTORY_RENDER_SCALE wants a mode from 1 to 15: left alone");
        None
    }
}

/// The replacement: `mov dword [rip+disp], mode; ret`, `disp` from the end
/// of the `mov` (10 bytes) to the mode.
fn replacement(mode: u32) -> [u8; 11] {
    let disp = (MODE_RVA as i64 - (SETTER_RVA as i64 + 10)) as i32;
    let mut r = [0u8; 11];
    r[0] = 0xc7;
    r[1] = 0x05;
    r[2..6].copy_from_slice(&disp.to_le_bytes());
    r[6..10].copy_from_slice(&mode.to_le_bytes());
    r[10] = 0xc3;
    r
}

/// Patches the setter of the image mapped at `base` if the switch asks for a
/// mode and the setter's bytes are the expected ones. Call before any title
/// code runs.
///
/// # Safety
/// `base` must be the mapped title image, with no title code running.
pub(crate) unsafe fn apply(base: usize) {
    let Some(mode) = asked() else { return };
    let at = base + SETTER_RVA;
    // SAFETY: the image is mapped; the setter and its padding lie in its code.
    let found = unsafe { std::slice::from_raw_parts(at as *const u8, SETTER.len() + PADDING) };
    if found[..SETTER.len()] != SETTER || found[SETTER.len()..].iter().any(|&b| b != 0xcc) {
        tracing::warn!(
            bytes = format_args!("{:02x?}", found),
            "VICTORY_RENDER_SCALE: the render-mode setter is not where this build has it: left alone"
        );
        return;
    }
    let new = replacement(mode);
    use windows_sys::Win32::System::Memory::{PAGE_EXECUTE_READWRITE, VirtualProtect};
    let mut old = 0u32;
    // SAFETY: the setter's own bytes and padding, made writable for the patch
    // and restored; no title code runs yet.
    unsafe {
        if VirtualProtect(at as *const _, new.len(), PAGE_EXECUTE_READWRITE, &mut old) == 0 {
            tracing::warn!("VICTORY_RENDER_SCALE: the setter's page could not be made writable");
            return;
        }
        std::ptr::copy_nonoverlapping(new.as_ptr(), at as *mut u8, new.len());
        VirtualProtect(at as *const _, new.len(), old, &mut old);
    }
    tracing::info!(mode, "VICTORY_RENDER_SCALE: the title's render-size mode is fixed (1 = half the output size, 2 = the title's two thirds)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replacement_stores_the_mode_where_the_setter_did() {
        // The original's displacement is from the end of its mov (6 bytes); the
        // replacement's from the end of its 10-byte mov: both reach the mode.
        let orig = i32::from_le_bytes([SETTER[2], SETTER[3], SETTER[4], SETTER[5]]) as i64;
        assert_eq!(SETTER_RVA as i64 + 6 + orig, MODE_RVA as i64);
        let r = replacement(1);
        let disp = i32::from_le_bytes([r[2], r[3], r[4], r[5]]) as i64;
        assert_eq!(SETTER_RVA as i64 + 10 + disp, MODE_RVA as i64);
        assert_eq!(&r[6..10], &1u32.to_le_bytes());
        assert_eq!(r[10], 0xc3);
        assert!(r.len() <= SETTER.len() + PADDING);
    }
}
