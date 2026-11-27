//! VIctory Launcher: one plain Windows window that builds VIctory from this repository and starts **your own copy**
//! of the title with `scripts\play-fsr.ps1` (the one place the play settings live). It ships nothing and downloads
//! nothing: it runs `cargo`, `target\diag\victory.exe shaders` and the play script, and shows their output.
//!
//! Put `VIctory-Launcher.exe` anywhere inside the folder you cloned (it looks for `scripts\play-fsr.ps1` in its own
//! folder and the folders above it). Your choices are kept in `%LOCALAPPDATA%\victory\launcher.txt`; saves and the
//! shader cache stay at VIctory's defaults under `%LOCALAPPDATA%\victory`.
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(not(windows))]
fn main() {
    eprintln!("VIctory Launcher runs on Windows only.");
}

#[cfg(windows)]
fn main() {
    app::run();
}

#[cfg(windows)]
mod app {
    use std::cell::RefCell;
    use std::fs::File;
    use std::io::{Read, Seek, SeekFrom};
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::{Child, Command, Stdio};

    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        COLOR_WINDOW, CreateFontIndirectW, GetSysColor, GetSysColorBrush, HBRUSH, HDC, HFONT, SetBkColor, SetTextColor,
    };
    use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Controls::{EM_REPLACESEL, EM_SETLIMITTEXT, EM_SETSEL};
    use windows::Win32::UI::HiDpi::{
        AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
        SystemParametersInfoForDpi,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{FOS_FILEMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH};
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::core::{HSTRING, PCWSTR, w};

    // Control ids.
    const GAME: i32 = 101;
    const GAME_BROWSE: i32 = 102;
    const GAME_OK: i32 = 103;
    const FSR_NOTE: i32 = 109;
    const USE_FSR: i32 = 110;
    const FSR: i32 = 111;
    const FSR_BROWSE: i32 = 112;
    const SERIES_S: i32 = 120;
    const SERIES_X: i32 = 121;
    const PANEL: i32 = 122;
    const ADVANCED: i32 = 130;
    const FULLSCREEN: i32 = 131;
    const LOW_MEMORY: i32 = 132;
    const SAFE: i32 = 133;
    const RAY_TRACING: i32 = 134;
    const EXTRA: i32 = 135;
    const EXTRA_LABEL: i32 = 136;
    const BUILD: i32 = 140;
    const SHADERS: i32 = 141;
    const PLAY: i32 = 142;
    const STATUS: i32 = 150;
    const LOG: i32 = 151;
    const TIMER: usize = 1;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// The window's controls (copied out of the cell before any call that may run the message loop).
    #[derive(Clone, Copy)]
    struct Ui {
        main: HWND,
        dpi: u32,
        advanced_open: bool,
    }

    /// A build or shader job running beside the window, its output going to `log`.
    struct Job {
        child: Child,
        log: PathBuf,
        read: u64,
    }

    thread_local! {
        static UI: RefCell<Option<Ui>> = const { RefCell::new(None) };
        static JOB: RefCell<Option<Job>> = const { RefCell::new(None) };
        static ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
    }

    fn ui() -> Ui {
        UI.with(|u| u.borrow().expect("the window exists"))
    }

    fn root() -> Option<PathBuf> {
        ROOT.with(|r| r.borrow().clone())
    }

    fn item(id: i32) -> HWND {
        // SAFETY: a child of our window by its id.
        unsafe { GetDlgItem(Some(ui().main), id) }.unwrap_or_default()
    }

    fn text(id: i32) -> String {
        let h = item(id);
        // SAFETY: plain queries on our own control; the buffer is as long as said.
        unsafe {
            let n = GetWindowTextLengthW(h).max(0) as usize;
            let mut buf = vec![0u16; n + 1];
            let got = GetWindowTextW(h, &mut buf).max(0) as usize;
            String::from_utf16_lossy(&buf[..got])
        }
    }

    fn set_text(id: i32, s: &str) {
        // SAFETY: our own control; the string lives through the call.
        let _ = unsafe { SetWindowTextW(item(id), &HSTRING::from(s)) };
    }

    fn checked(id: i32) -> bool {
        // SAFETY: a plain query on our own button.
        unsafe { SendMessageW(item(id), BM_GETCHECK, None, None) }.0 == 1
    }

    fn set_checked(id: i32, on: bool) {
        // SAFETY: our own button.
        unsafe { SendMessageW(item(id), BM_SETCHECK, Some(WPARAM(usize::from(on))), None) };
    }

    fn enable(id: i32, on: bool) {
        // SAFETY: our own control.
        let _ = unsafe { EnableWindow(item(id), on) };
    }

    fn show(id: i32, on: bool) {
        // SAFETY: our own control.
        let _ = unsafe { ShowWindow(item(id), if on { SW_SHOW } else { SW_HIDE }) };
    }

    fn message(body: &str) {
        // SAFETY: a modal box over our window; the strings live through the call.
        unsafe { MessageBoxW(Some(ui().main), &HSTRING::from(body), w!("VIctory Launcher"), MB_OK | MB_ICONINFORMATION) };
    }

    fn append_log(s: &str) {
        let h = item(LOG);
        let mut wide: Vec<u16> = s.replace("\r\n", "\n").replace('\n', "\r\n").encode_utf16().collect();
        wide.push(0);
        // SAFETY: our own edit control; the text lives through the calls.
        unsafe {
            let end = GetWindowTextLengthW(h).max(0) as usize;
            SendMessageW(h, EM_SETSEL, Some(WPARAM(end)), Some(LPARAM(end as isize)));
            SendMessageW(h, EM_REPLACESEL, Some(WPARAM(0)), Some(LPARAM(wide.as_ptr() as isize)));
        }
    }

    // --- remembered choices -----------------------------------------------------------------------------------------

    fn state_dir() -> PathBuf {
        std::env::var_os("LOCALAPPDATA").map_or_else(std::env::temp_dir, PathBuf::from).join("victory")
    }

    const REMEMBERED: [(&str, i32); 9] = [
        ("game", GAME),
        ("fsr", FSR),
        ("use_fsr", USE_FSR),
        ("series_x", SERIES_X),
        ("panel", PANEL),
        ("fullscreen", FULLSCREEN),
        ("low_memory", LOW_MEMORY),
        ("safe", SAFE),
        ("ray_tracing", RAY_TRACING),
    ];

    fn load_state() {
        let mut saved = std::collections::HashMap::new();
        if let Ok(t) = std::fs::read_to_string(state_dir().join("launcher.txt")) {
            for line in t.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    saved.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
        }
        set_checked(PANEL, true);
        set_checked(SERIES_S, true);
        for (key, id) in REMEMBERED {
            let Some(v) = saved.get(key) else { continue };
            match id {
                GAME | FSR => set_text(id, v),
                _ => set_checked(id, v == "1"),
            }
        }
        set_checked(SERIES_S, !checked(SERIES_X));
        if let Ok(extra) = std::fs::read_to_string(state_dir().join("launcher-extra.txt")) {
            set_text(EXTRA, &extra.replace("\r\n", "\n").replace('\n', "\r\n"));
        }
    }

    fn save_state() {
        let mut out = String::new();
        for (key, id) in REMEMBERED {
            let v = match id {
                GAME | FSR => text(id),
                _ => String::from(if checked(id) { "1" } else { "0" }),
            };
            out.push_str(&format!("{key}={v}\n"));
        }
        let _ = std::fs::create_dir_all(state_dir());
        let _ = std::fs::write(state_dir().join("launcher.txt"), out);
        let _ = std::fs::write(state_dir().join("launcher-extra.txt"), text(EXTRA));
    }

    // --- checks -----------------------------------------------------------------------------------------------------

    /// The folder holding `scripts\play-fsr.ps1`: this program's folder or one above it.
    fn find_root() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        exe.ancestors().skip(1).find(|d| d.join("scripts").join("play-fsr.ps1").is_file() && d.join("Cargo.toml").is_file()).map(Path::to_path_buf)
    }

    fn game_dir() -> String {
        text(GAME).trim().trim_matches('"').to_string()
    }

    fn game_exe() -> PathBuf {
        Path::new(&game_dir()).join("game_scarlett_master.exe")
    }

    fn victory_exe() -> Option<PathBuf> {
        root().map(|r| r.join("target").join("diag").join("victory.exe"))
    }

    fn game_running() -> bool {
        // SAFETY: a snapshot of the process list, walked and closed.
        unsafe {
            let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return false };
            let mut e = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
            let mut found = false;
            let mut more = Process32FirstW(snap, &mut e).is_ok();
            while more {
                let n = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                if String::from_utf16_lossy(&e.szExeFile[..n]).eq_ignore_ascii_case("victory.exe") {
                    found = true;
                    break;
                }
                more = Process32NextW(snap, &mut e).is_ok();
            }
            let _ = windows::Win32::Foundation::CloseHandle(snap);
            found
        }
    }

    fn update() {
        let busy = JOB.with(|j| j.borrow().is_some());
        let have_root = root().is_some();
        let game = game_dir();
        let exe_ok = !game.is_empty() && game_exe().is_file();
        let inside = exe_ok
            && root().is_some_and(|r| {
                let g = std::fs::canonicalize(&game).unwrap_or_else(|_| PathBuf::from(&game));
                let r = std::fs::canonicalize(&r).unwrap_or(r);
                g.starts_with(r)
            });
        set_text(
            GAME_OK,
            if game.is_empty() {
                ""
            } else if !exe_ok {
                "No game_scarlett_master.exe in that folder."
            } else if inside {
                "Move the game outside the VIctory-RTX folder: VIctory refuses game files inside it."
            } else {
                "Found game_scarlett_master.exe"
            },
        );
        let built = victory_exe().is_some_and(|p| p.is_file());
        set_text(BUILD, if built { "Rebuild" } else { "Build" });
        let ready = have_root && built && exe_ok && !inside;
        enable(BUILD, have_root && !busy);
        enable(SHADERS, ready && !busy);
        enable(PLAY, ready && !busy);
        let fsr_on = checked(USE_FSR);
        enable(FSR, fsr_on);
        enable(FSR_BROWSE, fsr_on);
        set_text(
            STATUS,
            if !have_root {
                "Put VIctory-Launcher.exe inside the folder you cloned VIctory-RTX into."
            } else if busy {
                "Working... (the output is below)"
            } else if !built {
                "Next: press Build (first time only, 5-15 minutes)."
            } else if !exe_ok || inside {
                "Pick your game folder."
            } else {
                "Ready. First time: Prepare shaders (about a minute), then PLAY."
            },
        );
    }

    // --- jobs -------------------------------------------------------------------------------------------------------

    fn start_job(what: &str, mut cmd: Command) {
        if JOB.with(|j| j.borrow().is_some()) {
            return;
        }
        let log = state_dir().join("launcher-last.log");
        let _ = std::fs::create_dir_all(state_dir());
        let (Ok(out), Ok(err)) = (File::create(&log), File::options().append(true).open(&log).or_else(|_| File::create(&log)))
        else {
            message("Could not write the log file in %LOCALAPPDATA%\\victory.");
            return;
        };
        append_log(&format!("--- {what} ---\n"));
        cmd.stdout(Stdio::from(out)).stderr(Stdio::from(err)).stdin(Stdio::null()).creation_flags(CREATE_NO_WINDOW);
        match cmd.spawn() {
            Ok(child) => {
                JOB.with(|j| *j.borrow_mut() = Some(Job { child, log, read: 0 }));
                // SAFETY: a timer on our own window.
                unsafe { SetTimer(Some(ui().main), TIMER, 700, None) };
            }
            Err(e) => append_log(&format!("--- could not start: {e} ---\n")),
        }
        update();
    }

    fn poll_job() {
        let (new, done) = JOB.with(|j| {
            let mut j = j.borrow_mut();
            let Some(job) = j.as_mut() else { return (String::new(), None) };
            let mut new = String::new();
            if let Ok(mut f) = File::open(&job.log)
                && f.seek(SeekFrom::Start(job.read)).is_ok()
            {
                let mut bytes = Vec::new();
                if f.read_to_end(&mut bytes).is_ok() {
                    job.read += bytes.len() as u64;
                    new = String::from_utf8_lossy(&bytes).into_owned();
                }
            }
            let done = job.child.try_wait().ok().flatten().map(|s| s.code().unwrap_or(-1));
            if done.is_some() {
                *j = None;
            }
            (new, done)
        });
        if !new.is_empty() {
            append_log(&new);
        }
        if let Some(code) = done {
            // SAFETY: our own timer.
            let _ = unsafe { KillTimer(Some(ui().main), TIMER) };
            append_log(&if code == 0 { "--- done ---\n".to_string() } else { format!("--- stopped with code {code} (see above) ---\n") });
            update();
        }
    }

    fn build() {
        let Some(root) = root() else { return };
        if Command::new("cargo").arg("--version").creation_flags(CREATE_NO_WINDOW).output().is_err() {
            message(
                "Rust is not installed (no 'cargo').\n\nInstall Rust from https://rustup.rs and the Visual Studio Build Tools \
                 with 'Desktop development with C++', then open the launcher again.",
            );
            return;
        }
        let mut cmd = Command::new("cargo");
        cmd.args(["build", "--locked", "--profile", "diag", "-p", "victory", "--features", "diagnostics"]).current_dir(root);
        start_job("Building VIctory (first time 5-15 minutes)", cmd);
    }

    fn shaders() {
        let (Some(root), Some(victory)) = (root(), victory_exe()) else { return };
        let mut cmd = Command::new(victory);
        cmd.arg("shaders").arg(Path::new(&game_dir()).join("shaders.rpf")).env("VICTORY_GAME_DIR", game_dir()).current_dir(root);
        start_job("Translating the game's shaders (about a minute)", cmd);
    }

    /// A PowerShell single-quoted string.
    fn quoted(s: &str) -> String {
        format!("'{}'", s.replace('\'', "''"))
    }

    fn play() {
        if game_running() {
            message("The game is already running. Close it first.");
            return;
        }
        let Some(root) = root() else { return };
        let fsr = text(FSR).trim().trim_matches('"').to_string();
        let use_fsr = checked(USE_FSR);
        if use_fsr && (fsr.is_empty() || !Path::new(&fsr).is_file()) {
            message("AMD FSR is ticked but no amd_fidelityfx_upscaler_dx12.dll is chosen.\n\nPick it, or untick AMD FSR for the best picture.");
            return;
        }
        save_state();
        let script = root.join("scripts").join("play-fsr.ps1");
        let mut line = format!("& {} -Exe {}", quoted(&script.to_string_lossy()), quoted(&game_exe().to_string_lossy()));
        if use_fsr {
            line += &format!(" -FsrDll {}", quoted(&fsr));
        }
        for (id, switch) in [(SERIES_X, "-SeriesX"), (FULLSCREEN, "-FullScreen"), (LOW_MEMORY, "-LowMemory"), (SAFE, "-Safe")] {
            if checked(id) {
                line += &format!(" {switch}");
            }
        }
        if !checked(PANEL) {
            line += " -NoPanel";
        }
        let mut more = Vec::new();
        if checked(RAY_TRACING) {
            more.push(("VICTORY_BVH_BUILDER".to_string(), "1".to_string()));
        }
        for l in text(EXTRA).lines() {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            match l.split_once('=') {
                Some((k, v)) if k.trim().starts_with("VICTORY_") && k.trim().chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_') => {
                    more.push((k.trim().to_string(), v.trim().to_string()));
                }
                _ => {
                    message(&format!("This extra setting is not of the form VICTORY_NAME=value:\n\n{l}"));
                    return;
                }
            }
        }
        if !more.is_empty() {
            let pairs: Vec<String> = more.iter().map(|(k, v)| format!("{k}={}", quoted(v))).collect();
            line += &format!(" -More @{{{}}}", pairs.join("; "));
        }
        let log = state_dir().join("launcher-play.log");
        let out = File::create(&log).ok();
        let mut cmd = Command::new("powershell");
        cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &line])
            .env("VICTORY_GAME_DIR", game_dir())
            .current_dir(&root)
            .stdin(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);
        if let Some(out) = out {
            if let Ok(err) = out.try_clone() {
                cmd.stderr(Stdio::from(err));
            }
            cmd.stdout(Stdio::from(out));
        }
        let fsr_note = if use_fsr { "AMD FSR on (softer picture)" } else { "AMD FSR off (best picture)" };
        let console = if checked(SERIES_X) { "X" } else { "S" };
        match cmd.spawn() {
            Ok(_) => {
                append_log(&format!("--- PLAY: Series {console}, {fsr_note}; the game's output goes to {} ---\n", log.display()));
                set_text(STATUS, "Starting... the game window opens in a moment (the first start of a build takes longer).");
            }
            Err(e) => append_log(&format!("--- could not start PowerShell: {e} ---\n")),
        }
    }

    fn pick(folder: bool) -> Option<String> {
        // SAFETY: the common file dialog, modal over our window; the path it returns is freed after it is copied.
        unsafe {
            let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
            let options = dialog.GetOptions().ok()?;
            dialog.SetOptions(options | if folder { FOS_PICKFOLDERS } else { FOS_FILEMUSTEXIST }).ok()?;
            if folder {
                dialog.SetTitle(w!("The folder that holds game_scarlett_master.exe")).ok()?;
            } else {
                dialog.SetTitle(w!("AMD's amd_fidelityfx_upscaler_dx12.dll")).ok()?;
                let types = [
                    COMDLG_FILTERSPEC { pszName: w!("AMD FSR upscaler"), pszSpec: w!("amd_fidelityfx_upscaler_dx12.dll") },
                    COMDLG_FILTERSPEC { pszName: w!("DLL files"), pszSpec: w!("*.dll") },
                ];
                dialog.SetFileTypes(&types).ok()?;
            }
            dialog.Show(Some(ui().main)).ok()?;
            let path = dialog.GetResult().ok()?.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
            let s = path.to_string().ok();
            CoTaskMemFree(Some(path.0 as *const core::ffi::c_void));
            s
        }
    }

    // --- the window -------------------------------------------------------------------------------------------------

    /// Pixels at the window's DPI for `v` at 96 DPI.
    fn px(v: i32) -> i32 {
        v * ui().dpi as i32 / 96
    }

    fn font(scale_num: i32, scale_den: i32, bold: bool) -> HFONT {
        // SAFETY: the system's message font, copied and made at a size; a plain struct for the query.
        unsafe {
            let mut m = NONCLIENTMETRICSW { cbSize: size_of::<NONCLIENTMETRICSW>() as u32, ..Default::default() };
            let _ = SystemParametersInfoForDpi(SPI_GETNONCLIENTMETRICS.0, m.cbSize, Some((&raw mut m).cast()), 0, ui().dpi);
            let mut lf = m.lfMessageFont;
            lf.lfHeight = lf.lfHeight * scale_num / scale_den;
            if bold {
                lf.lfWeight = 600;
            }
            CreateFontIndirectW(&lf)
        }
    }

    struct Place {
        x: i32,
        y: i32,
        w: i32,
        h: i32,
    }

    fn control(class: PCWSTR, label: &str, style: WINDOW_STYLE, id: i32, p: Place, f: HFONT) {
        let main = ui().main;
        // SAFETY: a child control of our window; the strings live through the call.
        unsafe {
            let Ok(h) = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                &HSTRING::from(label),
                WS_CHILD | WS_VISIBLE | style,
                px(p.x),
                px(p.y),
                px(p.w),
                px(p.h),
                Some(main),
                Some(HMENU(id as isize as *mut core::ffi::c_void)),
                None,
                None,
            ) else {
                return;
            };
            SendMessageW(h, WM_SETFONT, Some(WPARAM(f.0 as usize)), Some(LPARAM(1)));
        }
    }

    fn label(text: &str, id: i32, x: i32, y: i32, w: i32, h: i32, f: HFONT) {
        control(w!("STATIC"), text, WINDOW_STYLE(0), id, Place { x, y, w, h }, f);
    }
    fn button(text: &str, id: i32, x: i32, y: i32, w: i32, h: i32, f: HFONT) {
        control(w!("BUTTON"), text, WS_TABSTOP | WINDOW_STYLE(BS_PUSHBUTTON as u32), id, Place { x, y, w, h }, f);
    }
    fn check(text: &str, id: i32, x: i32, y: i32, w: i32, f: HFONT) {
        control(w!("BUTTON"), text, WS_TABSTOP | WINDOW_STYLE(BS_AUTOCHECKBOX as u32), id, Place { x, y, w, h: 24 }, f);
    }
    fn edit(id: i32, x: i32, y: i32, w: i32, f: HFONT) {
        control(w!("EDIT"), "", WS_TABSTOP | WS_BORDER | WINDOW_STYLE(ES_AUTOHSCROLL as u32), id, Place { x, y, w, h: 26 }, f);
    }

    const WIDTH: i32 = 660;
    const HEIGHT_CLOSED: i32 = 690;
    const HEIGHT_OPEN: i32 = 894;
    /// Where the part below the Advanced section starts, closed and open.
    const LOWER_CLOSED: i32 = 352;
    const LOWER_OPEN: i32 = 556;

    fn create_controls() {
        let normal = font(1, 1, false);
        let title = font(8, 5, true);
        let bold = font(1, 1, true);
        label("VIctory Launcher", 0, 20, 14, 600, 34, title);
        label("Your own copy of the game only. Nothing is downloaded or shipped by this launcher.", 0, 20, 50, 620, 22, normal);

        label("1.  Your game folder (the one with game_scarlett_master.exe)", 0, 20, 84, 620, 22, bold);
        edit(GAME, 20, 108, 500, normal);
        button("Browse...", GAME_BROWSE, 530, 107, 100, 28, normal);
        label("", GAME_OK, 20, 138, 620, 22, normal);

        label("2.  AMD FSR upscaling (optional)", 0, 20, 166, 620, 22, bold);
        label(
            "OFF = the best picture (recommended).  ON = a softer, less detailed picture for about the same speed \
             (measured: 17.7 against 17.3 fps) and a little more RAM.",
            FSR_NOTE,
            20,
            188,
            620,
            40,
            normal,
        );
        check("Use AMD FSR", USE_FSR, 20, 232, 130, normal);
        edit(FSR, 150, 230, 370, normal);
        button("Browse...", FSR_BROWSE, 530, 229, 100, 28, normal);
        label("amd_fidelityfx_upscaler_dx12.dll from AMD's FidelityFX SDK (not shipped here)", 0, 150, 260, 480, 20, normal);

        label("3.  How it runs", 0, 20, 290, 620, 22, bold);
        control(w!("BUTTON"), "Series S (faster)", WS_TABSTOP | WS_GROUP | WINDOW_STYLE(BS_AUTORADIOBUTTON as u32), SERIES_S, Place { x: 20, y: 314, w: 170, h: 24 }, normal);
        control(w!("BUTTON"), "Series X (sharper, slower)", WINDOW_STYLE(BS_AUTORADIOBUTTON as u32), SERIES_X, Place { x: 200, y: 314, w: 220, h: 24 }, normal);
        check("FPS panel (F3)", PANEL, 440, 314, 190, normal);

        button("Advanced  \u{25b8}", ADVANCED, 20, LOWER_CLOSED - 6 + 4, 140, 28, normal);
        // Advanced (hidden until opened).
        check("Full screen instead of a window", FULLSCREEN, 40, 392, 300, normal);
        check("Low-memory mode (cards under 8 GB; slower)", LOW_MEMORY, 40, 418, 400, normal);
        check("Safe mode (the first thing to try when something is wrong)", SAFE, 40, 444, 500, normal);
        check("Ray tracing (EXPERIMENTAL: loses the card within minutes on NVIDIA)", RAY_TRACING, 40, 470, 580, normal);
        label("Extra settings, one per line (VICTORY_NAME=value), for testers:", EXTRA_LABEL, 40, 498, 580, 20, normal);
        control(
            w!("EDIT"),
            "",
            WS_TABSTOP | WS_BORDER | WS_VSCROLL | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32),
            EXTRA,
            Place { x: 40, y: 520, w: 590, h: 0 },
            normal,
        );

        button("Build", BUILD, 20, 0, 140, 40, bold);
        button("Prepare shaders", SHADERS, 170, 0, 160, 40, bold);
        button("PLAY", PLAY, 490, 0, 140, 40, title);
        label("", STATUS, 20, 0, 620, 22, normal);
        control(
            w!("EDIT"),
            "",
            WS_BORDER | WS_VSCROLL | WINDOW_STYLE((ES_MULTILINE | ES_AUTOVSCROLL | ES_READONLY) as u32),
            LOG,
            Place { x: 20, y: 0, w: 610, h: 190 },
            font(9, 10, false),
        );
        // SAFETY: our own edit control.
        unsafe { SendMessageW(item(LOG), EM_SETLIMITTEXT, Some(WPARAM(4_000_000)), None) };
        layout();
    }

    /// Places the part below the Advanced section and shows or hides it.
    fn layout() {
        let open = ui().advanced_open;
        for id in [FULLSCREEN, LOW_MEMORY, SAFE, RAY_TRACING, EXTRA_LABEL, EXTRA] {
            show(id, open);
        }
        // The Extra box's height and the label above it.
        let move_to = |id: i32, x: i32, y: i32, w: i32, h: i32| {
            // SAFETY: our own control.
            let _ = unsafe { MoveWindow(item(id), px(x), px(y), px(w), px(h), true) };
        };
        move_to(EXTRA, 40, 520, 590, if open { 60 } else { 0 });
        let top = if open { LOWER_OPEN } else { LOWER_CLOSED };
        move_to(ADVANCED, 20, LOWER_CLOSED - 2, 140, 28);
        set_text(ADVANCED, if open { "Advanced  \u{25be}" } else { "Advanced  \u{25b8}" });
        move_to(BUILD, 20, top + 40, 140, 40);
        move_to(SHADERS, 170, top + 40, 160, 40);
        move_to(PLAY, 490, top + 40, 140, 40);
        move_to(STATUS, 20, top + 88, 620, 22);
        move_to(LOG, 20, top + 114, 610, 190);
        let height = if open { HEIGHT_OPEN } else { HEIGHT_CLOSED };
        let main = ui().main;
        // SAFETY: our own window: its client area made `WIDTH` x `height` at its DPI.
        unsafe {
            let mut r = windows::Win32::Foundation::RECT { left: 0, top: 0, right: px(WIDTH), bottom: px(height) };
            let style = WINDOW_STYLE(GetWindowLongW(main, GWL_STYLE) as u32);
            let _ = AdjustWindowRectExForDpi(&mut r, style, false, WINDOW_EX_STYLE(0), ui().dpi);
            let _ = SetWindowPos(main, None, 0, 0, r.right - r.left, r.bottom - r.top, SWP_NOMOVE | SWP_NOZORDER);
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(main), None, true);
        }
    }

    extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_COMMAND => {
                let id = (wparam.0 & 0xffff) as i32;
                let code = ((wparam.0 >> 16) & 0xffff) as u32;
                match id {
                    GAME if code == EN_CHANGE => update(),
                    GAME_BROWSE => {
                        if let Some(p) = pick(true) {
                            set_text(GAME, &p);
                        }
                    }
                    FSR_BROWSE => {
                        if let Some(p) = pick(false) {
                            set_text(FSR, &p);
                        }
                    }
                    USE_FSR => update(),
                    ADVANCED => {
                        UI.with(|u| {
                            if let Some(u) = u.borrow_mut().as_mut() {
                                u.advanced_open = !u.advanced_open;
                            }
                        });
                        layout();
                    }
                    BUILD => build(),
                    SHADERS => shaders(),
                    PLAY => play(),
                    _ => {}
                }
                LRESULT(0)
            }
            WM_TIMER => {
                poll_job();
                LRESULT(0)
            }
            WM_CTLCOLORSTATIC => {
                let hdc = HDC(wparam.0 as *mut core::ffi::c_void);
                let from = HWND(lparam.0 as *mut core::ffi::c_void);
                // SAFETY: the device context the control paints with, during this message.
                unsafe {
                    let color = if from == item(FSR_NOTE) {
                        COLORREF(0x0000_5ab0) // dark amber (BGR)
                    } else if from == item(GAME_OK) && text(GAME_OK).starts_with("Found") {
                        COLORREF(0x0030_8a1e) // green
                    } else if from == item(GAME_OK) {
                        COLORREF(0x0000_40c0) // orange-red
                    } else {
                        COLORREF(GetSysColor(windows::Win32::Graphics::Gdi::COLOR_WINDOWTEXT))
                    };
                    SetTextColor(hdc, color);
                    SetBkColor(hdc, COLORREF(GetSysColor(COLOR_WINDOW)));
                    LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize)
                }
            }
            WM_CLOSE => {
                save_state();
                // SAFETY: our own window.
                let _ = unsafe { DestroyWindow(hwnd) };
                LRESULT(0)
            }
            WM_DESTROY => {
                // SAFETY: ends our message loop.
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            // SAFETY: everything else as Windows does it.
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    pub fn run() {
        // SAFETY: start-up calls on this, the only thread with a window: DPI awareness, COM for the file dialog,
        // the window class and the window; then the message loop.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            ROOT.with(|r| *r.borrow_mut() = find_root());
            let instance = GetModuleHandleW(None).unwrap_or_default();
            let class = WNDCLASSW {
                lpfnWndProc: Some(proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
                hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as isize as *mut core::ffi::c_void),
                lpszClassName: w!("VIctoryLauncher"),
                ..Default::default()
            };
            RegisterClassW(&class);
            let Ok(main) = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("VIctoryLauncher"),
                w!("VIctory Launcher"),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                700,
                700,
                None,
                None,
                Some(instance.into()),
                None,
            ) else {
                return;
            };
            UI.with(|u| *u.borrow_mut() = Some(Ui { main, dpi: GetDpiForWindow(main).max(96), advanced_open: false }));
            create_controls();
            load_state();
            update();
            let _ = ShowWindow(main, SW_SHOW);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if IsDialogMessageW(main, &msg).as_bool() {
                    continue;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
