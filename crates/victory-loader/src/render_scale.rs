//! `VICTORY_RENDER_HALF=1`: the title renders at half its output size.
//!
//! Grand Theft Auto VI (Series 1.0.2.1) renders at a fraction of its output
//! size (3840x2160 on a Series X, whatever display mode we report) and its
//! upscaler makes the output image. The render size is set by the function
//! at RVA `0x1de4030` (into RVA `0x99fecf8`, `0x99fecfc`); it asks the
//! upscaler in use through its method at slot `0x50`:
//! - type 7 (RVA `0x28ef680`): the output size times 2/3 when its flag is
//!   set (the constant at RVA `0x615d95c`), 1 otherwise: 2560x1440 here
//!   (watched in run v133: `title+0x99fecf8 = 0xa00`, `+0x99fecfc = 0x5a0`);
//! - type 6 (RVA `0x7e9440`): times its own scale, at most 1 (the constant
//!   at RVA `0x5fc6ed8`) and at least 0.2.
//!
//! The mode table the size function reads first (RVA `0x85642b4`, 1 = 1/2)
//! does not decide it: the upscaler's answer overwrites it. With the switch,
//! both loads read the 0.5 the title keeps at RVA `0x5fc6edc` instead: type
//! 7 renders at half the output, type 6 at most at half. Each instruction is
//! changed only if its bytes are the ones of that build (logged otherwise).

/// The 0.5 (`0x3f000000`) in the title's read-only data.
const HALF_RVA: usize = 0x5fc_6edc;

/// A `vmovss xmm, [rip+disp]` (8 bytes, the displacement in the last four)
/// whose constant becomes [`HALF_RVA`]: its RVA, its original bytes.
const LOADS: [(usize, [u8; 8], &str); 2] = [
    (0x28e_f6b2, [0xc5, 0xfa, 0x10, 0x05, 0xa2, 0xe2, 0x86, 0x03], "upscaler type 7: 2/3"),
    (0x7e_9448, [0xc5, 0xfa, 0x10, 0x0d, 0x88, 0xda, 0x7d, 0x05], "upscaler type 6: at most 1"),
];

/// Whether the switch is on.
fn asked() -> bool {
    std::env::var_os("VICTORY_RENDER_HALF").is_some_and(|v| v == "1")
}

/// The bytes of a load at `rva` (original `orig`) that reads [`HALF_RVA`].
fn retargeted(rva: usize, orig: [u8; 8]) -> [u8; 8] {
    let disp = (HALF_RVA as i64 - (rva as i64 + 8)) as i32;
    let mut b = orig;
    b[4..8].copy_from_slice(&disp.to_le_bytes());
    b
}

/// Changes the loads of the image mapped at `base` if the switch is on and
/// their bytes are the expected ones. Call before any title code runs.
///
/// # Safety
/// `base` must be the mapped title image, with no title code running.
pub(crate) unsafe fn apply(base: usize) {
    if !asked() {
        return;
    }
    use windows_sys::Win32::System::Memory::{PAGE_EXECUTE_READWRITE, VirtualProtect};
    // SAFETY: the image is mapped; the constant lies in its read-only data.
    let half = unsafe { ((base + HALF_RVA) as *const f32).read_unaligned() };
    if half != 0.5 {
        tracing::warn!(found = half, "VICTORY_RENDER_HALF: no 0.5 where this build keeps it: left alone");
        return;
    }
    for (rva, orig, what) in LOADS {
        let at = base + rva;
        // SAFETY: eight bytes of the image's code.
        let found = unsafe { std::slice::from_raw_parts(at as *const u8, 8) };
        if found != orig {
            tracing::warn!(what, bytes = format_args!("{found:02x?}"), "VICTORY_RENDER_HALF: not this build's instruction: left alone");
            continue;
        }
        let new = retargeted(rva, orig);
        let mut old = 0u32;
        // SAFETY: the instruction's own bytes, made writable and restored; no
        // title code runs yet.
        unsafe {
            if VirtualProtect(at as *const _, 8, PAGE_EXECUTE_READWRITE, &mut old) == 0 {
                tracing::warn!(what, "VICTORY_RENDER_HALF: the page could not be made writable");
                continue;
            }
            std::ptr::copy_nonoverlapping(new.as_ptr(), at as *mut u8, 8);
            VirtualProtect(at as *const _, 8, old, &mut old);
        }
        tracing::info!(what, "VICTORY_RENDER_HALF: the upscaler's render scale reads 0.5");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retargeted_loads_read_the_half() {
        for (rva, orig, _) in LOADS {
            let b = retargeted(rva, orig);
            assert_eq!(b[..4], orig[..4]);
            let disp = i32::from_le_bytes([b[4], b[5], b[6], b[7]]) as i64;
            assert_eq!(rva as i64 + 8 + disp, HALF_RVA as i64);
        }
        // The originals read 2/3 and 1 (RVAs 0x615d95c, 0x5fc6ed8).
        let d7 = i32::from_le_bytes([0xa2, 0xe2, 0x86, 0x03]) as i64;
        assert_eq!(0x28e_f6b2 + 8 + d7, 0x615_d95c);
        let d6 = i32::from_le_bytes([0x88, 0xda, 0x7d, 0x05]) as i64;
        assert_eq!(0x7e_9448 + 8 + d6, 0x5fc_6ed8);
    }
}
