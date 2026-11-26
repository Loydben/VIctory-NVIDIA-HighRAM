//! Diagnostics: the run's numbers on the picture (`VICTORY_PANEL=1`, or
//! `=full` for the detailed card; off by default). F3 shows and hides it, F4
//! switches between the compact card (frame rate, frame time, a graph of the
//! last frames) and the detailed one (processor and card time, draws, memory),
//! while our window has the focus.
//!
//! LOCAL (Loydben, 2026-11-26): a see-through card in the upper left corner
//! instead of the black box of capitals: Windows' own fonts drawn with GDI
//! into a bitmap (grayscale antialiasing), laid on a rounded dark card of
//! `VICTORY_PANEL_OPACITY` (0.15 to 1, default 0.55), the frame rate in a
//! colour of its speed (green 30 and up, yellow 20, orange 14, red under),
//! and a bar for each of the last frames with the 30 fps line. The card is
//! made again four times a second from the counters of the frame-rate line,
//! memory and textures; a frame itself only blends it onto the swap chain's
//! buffer with a pass of two triangles (premultiplied alpha), behind the
//! title's picture: saved frames and the frame recorder see the title's own.

use std::collections::VecDeque;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{COLORREF, SIZE};
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::{D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP, ID3DBlob};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Gdi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_F3, VK_F4};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use super::{Host, HostList, transition};

/// How often the numbers change.
const EVERY: Duration = Duration::from_millis(250);

/// The texts are put into one of these in turn: the card reads the others,
/// at most a second old.
const UPLOADS: usize = 4;

/// Frame times kept for the graph.
const HISTORY: usize = 240;

/// The card's width and its compact height, in pixels of a 1080-line
/// picture; a detailed card adds [`ROW`] for each of [`DETAIL_ROWS`].
const CARD_W: f32 = 300.0;
const CARD_H: f32 = 112.0;
const ROW: f32 = 20.0;
const DETAIL_ROWS: usize = 4;
/// From the picture's corner.
const MARGIN: f32 = 16.0;

fn asked() -> Option<bool> {
    static ON: OnceLock<Option<bool>> = OnceLock::new();
    *ON.get_or_init(|| match std::env::var("VICTORY_PANEL").ok()?.trim() {
        "full" | "detailed" | "2" => Some(true),
        "1" | "on" | "compact" => Some(false),
        _ => None,
    })
}

/// Whether it is shown now.
fn shown() -> &'static AtomicBool {
    static SHOWN: OnceLock<AtomicBool> = OnceLock::new();
    SHOWN.get_or_init(|| AtomicBool::new(asked().is_some()))
}

/// Whether the detailed card is shown.
fn detailed() -> &'static AtomicBool {
    static FULL: OnceLock<AtomicBool> = OnceLock::new();
    FULL.get_or_init(|| AtomicBool::new(asked() == Some(true)))
}

fn opacity() -> f32 {
    static O: OnceLock<f32> = OnceLock::new();
    *O.get_or_init(|| {
        std::env::var("VICTORY_PANEL_OPACITY")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .map_or(0.55, |v| v.clamp(0.15, 1.0))
    })
}

/// `key` was pressed since the last call, with one of our windows in front.
fn pressed(key: u16, down: &AtomicBool) -> bool {
    // SAFETY: a plain query.
    let now = unsafe { GetAsyncKeyState(i32::from(key)) } < 0;
    if down.swap(now, Ordering::Relaxed) || !now {
        return false;
    }
    let mut process = 0;
    // SAFETY: plain queries; room for the answer.
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), &mut process) };
    process == std::process::id()
}

/// The counters read for it. They grow, and go back to 0 with the frame-rate
/// line (right behind a look of ours).
#[derive(Clone, Copy, Default)]
struct Counters([u64; COUNTERS]);

const COUNTERS: usize = 11;

impl Counters {
    fn now() -> Counters {
        use super::{CARD_CLOCK_US, COPY_BACK, PREPARING};
        use crate::copystat::{C, peek};
        let get = |a: &std::sync::atomic::AtomicU64| a.load(Ordering::Relaxed);
        Counters([
            super::worked(),
            get(&CARD_CLOCK_US[0]),
            get(&CARD_CLOCK_US[1]),
            get(&CARD_CLOCK_US[2]),
            get(&PREPARING[3]),
            get(&COPY_BACK[0]),
            get(&COPY_BACK[1]),
            get(&crate::shadow::CARD_STATS[0]),
            crate::draw::dropped_now(),
            peek(C::RecordNs),
            crate::draw::drawn(),
        ])
    }

    /// What came since `before`, per frame.
    fn since(self, before: Counters, frames: f64) -> [f64; COUNTERS] {
        let mut out = [0.0; COUNTERS];
        for (i, out) in out.iter_mut().enumerate() {
            let (was, is) = (before.0[i], self.0[i]);
            *out = if is >= was { is - was } else { is } as f64 / frames;
        }
        out
    }
}

/// What the card shows.
#[derive(Clone, Default)]
struct Stats {
    /// A frame's time, the longest frame's (milliseconds).
    ms: f64,
    worst: f64,
    /// The submitting thread's work and the card's busy time a frame (ms).
    cpu: f64,
    gpu: f64,
    copy_back: f64,
    draws: f64,
    dropped: f64,
    mb_up: f64,
    /// Gigabytes: the card's memory used and its budget, the process's own.
    vram: (f64, f64),
    ram: f64,
    /// What this takes of a frame, in microseconds.
    cost: f64,
}

impl Stats {
    fn read(c: &[f64; COUNTERS], ms: f64, worst: f64, cost: f64, host: &Host) -> Stats {
        let gb = |bytes: u64| bytes as f64 / (1u64 << 30) as f64;
        let (_, private) = crate::copystat::process_memory();
        let [budget, used, _, _] = host.video_memory().unwrap_or_default();
        let idle = (c[1] / 1e3).min(ms);
        Stats {
            ms,
            worst,
            cpu: c[0] / 1e3,
            gpu: ms - idle,
            copy_back: c[5] / 1e3,
            draws: c[10],
            dropped: c[8],
            mb_up: c[7] / 1e6,
            vram: (gb(used), gb(budget)),
            ram: gb(private),
            cost,
        }
    }
}

/// The colour of a frame rate (and of a frame of that time).
fn speed_colour(fps: f64) -> [u8; 3] {
    if fps >= 29.5 {
        [92, 230, 140]
    } else if fps >= 20.0 {
        [255, 210, 80]
    } else if fps >= 14.0 {
        [255, 150, 60]
    } else {
        [255, 92, 92]
    }
}

/// A premultiplied RGBA picture being painted.
struct Canvas {
    w: usize,
    h: usize,
    px: Vec<[f32; 4]>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Canvas {
        Canvas { w, h, px: vec![[0.0; 4]; w * h] }
    }

    /// `colour` at `alpha` over the pixel, `cover` of it.
    fn blend(&mut self, x: usize, y: usize, colour: [u8; 3], alpha: f32) {
        if x >= self.w || y >= self.h || alpha <= 0.0 {
            return;
        }
        let p = &mut self.px[y * self.w + x];
        let a = alpha.min(1.0);
        for i in 0..3 {
            p[i] = f32::from(colour[i]) / 255.0 * a + p[i] * (1.0 - a);
        }
        p[3] = a + p[3] * (1.0 - a);
    }

    /// A rectangle, `alpha` throughout.
    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, colour: [u8; 3], alpha: f32) {
        let (x0, y0) = (x.max(0.0) as usize, y.max(0.0) as usize);
        let (x1, y1) = (((x + w).ceil() as usize).min(self.w), ((y + h).ceil() as usize).min(self.h));
        for yy in y0..y1 {
            for xx in x0..x1 {
                // (partly covered edge pixels)
                let cx = ((xx as f32 + 1.0).min(x + w) - (xx as f32).max(x)).clamp(0.0, 1.0);
                let cy = ((yy as f32 + 1.0).min(y + h) - (yy as f32).max(y)).clamp(0.0, 1.0);
                self.blend(xx, yy, colour, alpha * cx * cy);
            }
        }
    }

    /// The whole canvas as a rounded rectangle of `radius`, `alpha` inside,
    /// with a faint rim.
    fn card(&mut self, radius: f32, colour: [u8; 3], alpha: f32, rim: f32) {
        let (w, h) = (self.w as f32, self.h as f32);
        for y in 0..self.h {
            for x in 0..self.w {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let qx = (px - w / 2.0).abs() - (w / 2.0 - radius);
                let qy = (py - h / 2.0).abs() - (h / 2.0 - radius);
                let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - radius;
                let inside = (0.5 - outside).clamp(0.0, 1.0);
                if inside <= 0.0 {
                    continue;
                }
                self.blend(x, y, colour, alpha * inside);
                // the rim: a pixel wide just inside the edge
                let edge = (1.0 - (outside + 1.0).abs()).clamp(0.0, 1.0);
                self.blend(x, y, [255, 255, 255], rim * edge);
            }
        }
    }

    /// Text drawn by GDI in `colour` on black (`bgra`), its coverage the
    /// brightest channel, laid over the canvas.
    fn text_layer(&mut self, bgra: &[u8]) {
        for (i, p) in bgra.chunks_exact(4).enumerate().take(self.w * self.h) {
            let (b, g, r) = (f32::from(p[0]), f32::from(p[1]), f32::from(p[2]));
            let a = r.max(g).max(b) / 255.0;
            if a <= 0.0 {
                continue;
            }
            let q = &mut self.px[i];
            q[0] = r / 255.0 + q[0] * (1.0 - a);
            q[1] = g / 255.0 + q[1] * (1.0 - a);
            q[2] = b / 255.0 + q[2] * (1.0 - a);
            q[3] = a + q[3] * (1.0 - a);
        }
    }

    /// Rows of RGBA8 (premultiplied), `pitch` bytes apart.
    fn bytes(&self, pitch: usize) -> Vec<u8> {
        let mut out = vec![0u8; pitch * self.h];
        for y in 0..self.h {
            for x in 0..self.w {
                let p = self.px[y * self.w + x];
                let at = y * pitch + x * 4;
                for i in 0..4 {
                    out[at + i] = (p[i].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                }
            }
        }
        out
    }
}

/// One piece of text for [`Gdi::draw`].
struct Text<'a> {
    x: i32,
    y: i32,
    /// Font size in pixels, semibold or not.
    px: i32,
    bold: bool,
    colour: [u8; 3],
    s: &'a str,
    /// Right-aligned on `x`.
    right: bool,
}

/// Texts drawn with GDI into a top-down 32-bit bitmap of `w` x `h`: its BGRA
/// bytes, black where there is no text.
fn gdi_text(w: usize, h: usize, texts: &[Text]) -> Option<Vec<u8>> {
    // SAFETY: plain GDI calls on a memory DC and a DIB section of ours, all
    // released before returning; the bits are read after GdiFlush.
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return None;
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let Ok(bitmap) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(dc);
            return None;
        };
        let old_bitmap = SelectObject(dc, bitmap.into());
        std::ptr::write_bytes(bits.cast::<u8>(), 0, w * h * 4);
        SetBkMode(dc, TRANSPARENT);
        for t in texts {
            let face: Vec<u16> = if t.bold { "Segoe UI Semibold" } else { "Segoe UI" }.encode_utf16().chain([0]).collect();
            let font = CreateFontW(
                -t.px,
                0,
                0,
                0,
                if t.bold { FW_SEMIBOLD.0 as i32 } else { FW_NORMAL.0 as i32 },
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                ANTIALIASED_QUALITY,
                0,
                windows::core::PCWSTR(face.as_ptr()),
            );
            let old_font = SelectObject(dc, font.into());
            let [r, g, b] = t.colour;
            SetTextColor(dc, COLORREF(u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16));
            let wide: Vec<u16> = t.s.encode_utf16().collect();
            let mut x = t.x;
            if t.right {
                let mut size = SIZE::default();
                let _ = GetTextExtentPoint32W(dc, &wide, &mut size);
                x -= size.cx;
            }
            let _ = TextOutW(dc, x, t.y, &wide);
            SelectObject(dc, old_font);
            let _ = DeleteObject(font.into());
        }
        let _ = GdiFlush();
        let out = std::slice::from_raw_parts(bits.cast::<u8>(), w * h * 4).to_vec();
        SelectObject(dc, old_bitmap);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(dc);
        Some(out)
    }
}

/// The card at `s` pixels to a 1080-line pixel: its size and RGBA8 rows
/// (premultiplied), `pitch` bytes apart.
fn paint(st: &Stats, history: &VecDeque<f32>, full: bool, s: f32, pitch: impl Fn(usize) -> usize) -> (u32, u32, usize, Vec<u8>) {
    let w = (CARD_W * s).round() as usize;
    let h = ((CARD_H + if full { ROW * DETAIL_ROWS as f32 + 6.0 } else { 0.0 }) * s).round() as usize;
    let mut c = Canvas::new(w, h);
    let op = opacity();
    c.card(10.0 * s, [14, 17, 22], op, 0.10);
    let fps = 1e3 / st.ms.max(0.001);
    let accent = speed_colour(fps);
    // an accent bar along the left edge
    c.rect(0.0, 12.0 * s, 3.0 * s, h as f32 - 24.0 * s, accent, 0.95);
    // the graph: one bar a frame, the newest on the right
    let (gx, gy, gw, gh) = (14.0 * s, 64.0 * s, w as f32 - 28.0 * s, 36.0 * s);
    c.rect(gx, gy, gw, gh, [255, 255, 255], 0.05);
    let step = (2.0 * s).max(1.0);
    let bars = (gw / step) as usize;
    let mut sorted: Vec<f32> = history.iter().copied().collect();
    sorted.sort_by(f32::total_cmp);
    let p95 = sorted.get(sorted.len() * 95 / 100).copied().unwrap_or(33.3);
    let top = (p95 * 1.25).clamp(50.0, 150.0);
    for (i, &ft) in history.iter().rev().take(bars).enumerate() {
        let bh = (ft / top).min(1.0) * gh;
        let x = gx + gw - (i as f32 + 1.0) * step;
        c.rect(x, gy + gh - bh, (step - (0.6 * s).max(0.5)).max(0.75), bh, speed_colour(1e3 / f64::from(ft.max(0.1))), 0.85);
    }
    // the 30 fps line
    let y30 = gy + gh - (33.3 / top) * gh;
    let mut x = gx;
    while x < gx + gw {
        c.rect(x, y30, (4.0 * s).min(gx + gw - x), (1.0 * s).max(1.0), [255, 255, 255], 0.45);
        x += 7.0 * s;
    }
    let px = |v: f32| (v * s).round() as i32;
    let fps_text = format!("{:.0}", fps);
    let ms_text = format!("{:.1} ms", st.ms);
    let worst_text = format!("worst {:.0} ms", st.worst);
    let label = [168, 178, 192];
    let white = [242, 245, 250];
    let mut texts = vec![
        Text { x: px(14.0), y: px(6.0), px: px(40.0), bold: true, colour: accent, s: &fps_text, right: false },
        Text { x: px(14.0) + px(40.0) * fps_text.len() as i32 * 56 / 100 + px(6.0), y: px(28.0), px: px(14.0), bold: true, colour: label, s: "FPS", right: false },
        Text { x: (w as i32) - px(14.0), y: px(12.0), px: px(18.0), bold: true, colour: white, s: &ms_text, right: true },
        Text { x: (w as i32) - px(14.0), y: px(36.0), px: px(12.5), bold: false, colour: label, s: &worst_text, right: true },
    ];
    let rows: Vec<(String, String, String, String)> = vec![
        ("CPU".into(), format!("{:.1} ms", st.cpu), "GPU".into(), format!("{:.1} ms", st.gpu)),
        ("Draws".into(), format!("{:.0}", st.draws), "Skipped".into(), format!("{:.0}", st.dropped)),
        ("VRAM".into(), format!("{:.1} / {:.1} GB", st.vram.0, st.vram.1), "RAM".into(), format!("{:.1} GB", st.ram)),
        ("Copy back".into(), format!("{:.1} ms", st.copy_back), "Upload".into(), format!("{:.0} MB", st.mb_up)),
    ];
    if full {
        let y0 = CARD_H + 2.0;
        // a thin divider
        c.rect(14.0 * s, (y0 - 4.0) * s, w as f32 - 28.0 * s, (1.0 * s).max(1.0), [255, 255, 255], 0.10);
        for (i, (l1, v1, l2, v2)) in rows.iter().enumerate() {
            let y = px(y0 + ROW * i as f32);
            let half = (w as i32) / 2;
            texts.push(Text { x: px(14.0), y, px: px(13.0), bold: false, colour: label, s: l1, right: false });
            texts.push(Text { x: half - px(10.0), y, px: px(13.0), bold: true, colour: white, s: v1, right: true });
            texts.push(Text { x: half + px(10.0), y, px: px(13.0), bold: false, colour: label, s: l2, right: false });
            texts.push(Text { x: (w as i32) - px(14.0), y, px: px(13.0), bold: true, colour: white, s: v2, right: true });
        }
    }
    let _ = st.cost;
    if let Some(bgra) = gdi_text(w, h, &texts) {
        c.text_layer(&bgra);
    }
    let pitch = pitch(w);
    (w as u32, h as u32, pitch, c.bytes(pitch))
}

const SOURCE: &str = r#"
#define RS "RootConstants(num32BitConstants=6, b0), DescriptorTable(SRV(t0))"
cbuffer Place : register(b0) { float4 ndc; float2 origin; };
Texture2D<float4> card : register(t0);
struct V { float4 pos : SV_Position; };
[RootSignature(RS)]
V vs(uint id : SV_VertexID) {
    float2 t = float2(id & 1, id >> 1);
    V o;
    o.pos = float4(lerp(ndc.x, ndc.z, t.x), lerp(ndc.y, ndc.w, t.y), 0, 1);
    return o;
}
[RootSignature(RS)]
float4 ps(V i) : SV_Target {
    return card.Load(int3(int2(i.pos.xy - origin), 0));
}
"#;

fn compile(entry: windows::core::PCSTR, target: windows::core::PCSTR) -> Option<Vec<u8>> {
    let mut code: Option<ID3DBlob> = None;
    // SAFETY: the source and names live through the call; the out pointer is ours.
    unsafe {
        D3DCompile(
            SOURCE.as_ptr().cast(),
            SOURCE.len(),
            windows::core::s!("panel"),
            None,
            None,
            entry,
            target,
            0,
            0,
            &mut code,
            None,
        )
        .ok()?;
        let code = code?;
        Some(std::slice::from_raw_parts(code.GetBufferPointer().cast::<u8>(), code.GetBufferSize()).to_vec())
    }
}

/// The pass that blends the card on: its root signature and a pipeline for
/// each swap chain format met.
struct Pass {
    root: ID3D12RootSignature,
    vs: Vec<u8>,
    ps: Vec<u8>,
    pipelines: Vec<(DXGI_FORMAT, ID3D12PipelineState)>,
    views: ID3D12DescriptorHeap,
    targets: ID3D12DescriptorHeap,
}

impl Pass {
    fn new(host: &Host) -> Option<Pass> {
        let vs = compile(windows::core::s!("vs"), windows::core::s!("vs_5_1"))?;
        let ps = compile(windows::core::s!("ps"), windows::core::s!("ps_5_1"))?;
        // SAFETY: the shader's own bytes (the root signature is inside it); plain creation.
        unsafe {
            let root: ID3D12RootSignature = host.device.CreateRootSignature(0, &vs).ok()?;
            let views: ID3D12DescriptorHeap = host
                .device
                .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                    NumDescriptors: 1,
                    Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                    NodeMask: 0,
                })
                .ok()?;
            let targets: ID3D12DescriptorHeap = host
                .device
                .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                    Type: D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                    NumDescriptors: 1,
                    Flags: D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                    NodeMask: 0,
                })
                .ok()?;
            Some(Pass { root, vs, ps, pipelines: Vec::new(), views, targets })
        }
    }

    fn pipeline(&mut self, host: &Host, format: DXGI_FORMAT) -> Option<ID3D12PipelineState> {
        if let Some((_, p)) = self.pipelines.iter().find(|(f, _)| *f == format) {
            return Some(p.clone());
        }
        let code = |b: &[u8]| D3D12_SHADER_BYTECODE { pShaderBytecode: b.as_ptr().cast(), BytecodeLength: b.len() };
        let mut blend = D3D12_BLEND_DESC::default();
        blend.RenderTarget[0] = D3D12_RENDER_TARGET_BLEND_DESC {
            BlendEnable: true.into(),
            LogicOpEnable: false.into(),
            SrcBlend: D3D12_BLEND_ONE,
            DestBlend: D3D12_BLEND_INV_SRC_ALPHA,
            BlendOp: D3D12_BLEND_OP_ADD,
            SrcBlendAlpha: D3D12_BLEND_ONE,
            DestBlendAlpha: D3D12_BLEND_INV_SRC_ALPHA,
            BlendOpAlpha: D3D12_BLEND_OP_ADD,
            LogicOp: D3D12_LOGIC_OP_NOOP,
            RenderTargetWriteMask: D3D12_COLOR_WRITE_ENABLE_ALL.0 as u8,
        };
        let mut formats = [DXGI_FORMAT_UNKNOWN; 8];
        formats[0] = format;
        // SAFETY: plain creation from our own shaders and root signature.
        let pipeline: ID3D12PipelineState = unsafe {
            let desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
                pRootSignature: std::mem::transmute_copy(&self.root),
                VS: code(&self.vs),
                PS: code(&self.ps),
                BlendState: blend,
                SampleMask: u32::MAX,
                RasterizerState: D3D12_RASTERIZER_DESC {
                    FillMode: D3D12_FILL_MODE_SOLID,
                    CullMode: D3D12_CULL_MODE_NONE,
                    DepthClipEnable: true.into(),
                    ..Default::default()
                },
                PrimitiveTopologyType: D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
                NumRenderTargets: 1,
                RTVFormats: formats,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                ..Default::default()
            };
            host.device.CreateGraphicsPipelineState(&desc).ok()?
        };
        self.pipelines.push((format, pipeline.clone()));
        Some(pipeline)
    }
}

/// The card's texture and the upload buffers it is filled from.
struct Buffers {
    size: (u32, u32),
    pitch: u32,
    uploads: Vec<(ID3D12Resource, *mut u8)>,
    texture: ID3D12Resource,
    /// The upload written last.
    turn: usize,
    /// It is not in the texture yet.
    fresh: bool,
    /// The texture can be read by the pass (else it is to be copied into).
    readable: bool,
}

// SAFETY: free-threaded resources, and memory only written under the presenter's lock.
unsafe impl Send for Buffers {}

impl Buffers {
    fn new(host: &Host, pass: &Pass, (w, h): (u32, u32)) -> Option<Buffers> {
        let pitch = (w * 4).next_multiple_of(256);
        let bytes = u64::from(pitch) * u64::from(h);
        let uploads = (0..UPLOADS).map(|_| host.upload_buffer(bytes)).collect::<Option<Vec<_>>>()?;
        let heap = D3D12_HEAP_PROPERTIES { Type: D3D12_HEAP_TYPE_DEFAULT, ..Default::default() };
        let desc = D3D12_RESOURCE_DESC {
            Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            Width: u64::from(w),
            Height: h,
            DepthOrArraySize: 1,
            MipLevels: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
            ..Default::default()
        };
        let mut texture: Option<ID3D12Resource> = None;
        // SAFETY: plain creation of a texture and a view of it in our heap.
        unsafe {
            host.device
                .CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &desc, D3D12_RESOURCE_STATE_COPY_DEST, None, &mut texture)
                .ok()?;
            let texture = texture.as_ref()?;
            host.device.CreateShaderResourceView(
                texture,
                Some(&D3D12_SHADER_RESOURCE_VIEW_DESC {
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
                    Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
                    Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
                        Texture2D: D3D12_TEX2D_SRV { MostDetailedMip: 0, MipLevels: 1, ..Default::default() },
                    },
                }),
                pass.views.GetCPUDescriptorHandleForHeapStart(),
            );
        }
        Some(Buffers { size: (w, h), pitch, uploads, texture: texture?, turn: 0, fresh: false, readable: false })
    }

    fn write(&mut self, picture: &[u8]) {
        self.turn = (self.turn + 1) % UPLOADS;
        let len = picture.len().min(self.pitch as usize * self.size.1 as usize);
        // SAFETY: a mapped upload buffer of the picture's size, which the card read a while ago.
        unsafe { std::ptr::copy_nonoverlapping(picture.as_ptr(), self.uploads[self.turn].1, len) };
        self.fresh = true;
    }
}

/// The panel of a presenter.
pub(super) struct Panel {
    since: Instant,
    frames: u32,
    last: Instant,
    worst: f64,
    /// What this took since the last text.
    cost: Duration,
    before: Counters,
    stats: Option<Stats>,
    history: VecDeque<f32>,
    changed: bool,
    /// The card painted last, for the scale and form it was painted at.
    painted: Option<(f32, bool, (u32, u32, usize, Vec<u8>))>,
    pass: Option<Pass>,
    buffers: Option<Buffers>,
    failed: bool,
}

impl Panel {
    pub(super) fn new() -> Panel {
        let now = Instant::now();
        Panel {
            since: now,
            frames: 0,
            last: now,
            worst: 0.0,
            cost: Duration::ZERO,
            before: Counters::default(),
            stats: None,
            history: VecDeque::with_capacity(HISTORY),
            changed: false,
            painted: None,
            pass: None,
            buffers: None,
            failed: false,
        }
    }

    /// A frame was presented; `line`: the counters go back to 0 right behind
    /// this call. (Not shown it asks for the keys' state and no more.)
    pub(super) fn frame(&mut self, line: bool) {
        static F3: AtomicBool = AtomicBool::new(false);
        static F4: AtomicBool = AtomicBool::new(false);
        if pressed(VK_F3, &F3) {
            shown().fetch_xor(true, Ordering::Relaxed);
        }
        if pressed(VK_F4, &F4) {
            detailed().fetch_xor(true, Ordering::Relaxed);
            self.changed = true;
        }
        if !shown().load(Ordering::Relaxed) {
            self.frames = 0;
            return;
        }
        let now = Instant::now();
        if self.frames == 0 && now - self.last > 2 * EVERY {
            // (Shown from here on.)
            (self.since, self.last, self.worst, self.before) = (now, now, 0.0, Counters::now());
            self.frames = 0;
            self.history.clear();
        }
        let dt = (now - self.last).as_secs_f64() * 1e3;
        self.frames += 1;
        self.worst = self.worst.max(dt);
        self.last = now;
        if dt > 0.0 {
            if self.history.len() == HISTORY {
                self.history.pop_front();
            }
            self.history.push_back(dt as f32);
        }
        let elapsed = now - self.since;
        if elapsed < EVERY && !(line && elapsed * 2 >= EVERY) {
            return;
        }
        let Some(host) = super::host() else { return };
        let frames = f64::from(self.frames);
        let counters = Counters::now();
        let c = counters.since(self.before, frames);
        self.before = counters;
        let cost = self.cost.as_secs_f64() * 1e6 / frames;
        self.stats = Some(Stats::read(&c, elapsed.as_secs_f64() * 1e3 / frames, self.worst, cost, host));
        self.changed = true;
        (self.since, self.frames, self.worst) = (now, 0, 0.0);
        self.cost = now.elapsed();
    }

    /// Records the blend of the card onto `back`, a buffer of the swap chain
    /// of that size and format in the state to be copied into (and leaves it
    /// in that state).
    pub(super) fn draw(
        &mut self,
        host: &Host,
        list: &HostList,
        back: &ID3D12Resource,
        (width, height): (u32, u32),
        format: DXGI_FORMAT,
    ) {
        if !shown().load(Ordering::Relaxed) || self.failed {
            return;
        }
        let Some(stats) = self.stats.clone() else { return };
        let stats = &stats;
        let began = Instant::now();
        // Not the title's: the frame recorder leaves it out.
        let _ours = crate::replay::record::unrecorded();
        if self.pass.is_none() {
            self.pass = Pass::new(host);
            if self.pass.is_none() {
                tracing::warn!("panel: its pass could not be made; not shown");
                self.failed = true;
                return;
            }
        }
        let s = (height as f32 / 1080.0).clamp(0.6, 3.0);
        let full = detailed().load(Ordering::Relaxed);
        if std::mem::take(&mut self.changed) || self.painted.as_ref().is_none_or(|p| p.0 != s || p.1 != full) {
            let picture = paint(stats, &self.history, full, s, |w| (w * 4).next_multiple_of(256));
            self.painted = Some((s, full, picture));
            let (w, h) = {
                let p = &self.painted.as_ref().unwrap().2;
                (p.0, p.1)
            };
            if self.buffers.as_ref().is_none_or(|b| b.size != (w, h)) {
                // (What the card may still read stays: the size changes rarely.)
                std::mem::forget(self.buffers.take());
                self.buffers = Buffers::new(host, self.pass.as_ref().unwrap(), (w, h));
                if self.buffers.is_none() {
                    self.failed = true;
                    return;
                }
            }
            let bytes = &self.painted.as_ref().unwrap().2.3;
            self.buffers.as_mut().unwrap().write(bytes);
        }
        let (Some(pass), Some(buffers)) = (self.pass.as_mut(), self.buffers.as_mut()) else { return };
        let Some(pipeline) = pass.pipeline(host, format) else {
            self.failed = true;
            return;
        };
        let (w, h) = buffers.size;
        let (x0, y0) = ((MARGIN * s).round(), (MARGIN * s).round());
        let (x1, y1) = ((x0 + w as f32).min(width as f32), (y0 + h as f32).min(height as f32));
        let ndc = |x: f32, y: f32| (x / width as f32 * 2.0 - 1.0, 1.0 - y / height as f32 * 2.0);
        let (l, t) = ndc(x0, y0);
        let (r, b) = ndc(x1, y1);
        let constants: [f32; 6] = [l, t, r, b, x0, y0];
        let cl = &list.list;
        // SAFETY: an open list (the present's) and live objects of ours; the
        // back buffer is in the copy-destination state and goes back to it.
        unsafe {
            if std::mem::take(&mut buffers.fresh) {
                if buffers.readable {
                    cl.ResourceBarrier(&[transition(
                        &buffers.texture,
                        D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                        D3D12_RESOURCE_STATE_COPY_DEST,
                    )]);
                }
                let from = D3D12_TEXTURE_COPY_LOCATION {
                    pResource: std::mem::transmute_copy(&buffers.uploads[buffers.turn].0),
                    Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
                    Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                        PlacedFootprint: D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
                            Offset: 0,
                            Footprint: D3D12_SUBRESOURCE_FOOTPRINT {
                                Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                                Width: w,
                                Height: h,
                                Depth: 1,
                                RowPitch: buffers.pitch,
                            },
                        },
                    },
                };
                let to = D3D12_TEXTURE_COPY_LOCATION {
                    pResource: std::mem::transmute_copy(&buffers.texture),
                    Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
                    Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: 0 },
                };
                cl.CopyTextureRegion(&to, 0, 0, 0, &from, None);
                cl.ResourceBarrier(&[transition(
                    &buffers.texture,
                    D3D12_RESOURCE_STATE_COPY_DEST,
                    D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                )]);
                buffers.readable = true;
            }
            let rtv = pass.targets.GetCPUDescriptorHandleForHeapStart();
            host.device.CreateRenderTargetView(back, None, rtv);
            cl.ResourceBarrier(&[transition(back, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_RENDER_TARGET)]);
            cl.OMSetRenderTargets(1, Some(&rtv), false, None);
            cl.RSSetViewports(&[D3D12_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: width as f32,
                Height: height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]);
            cl.RSSetScissorRects(&[windows::Win32::Foundation::RECT { left: 0, top: 0, right: width as i32, bottom: height as i32 }]);
            cl.SetDescriptorHeaps(&[Some(pass.views.clone())]);
            cl.SetGraphicsRootSignature(&pass.root);
            cl.SetPipelineState(&pipeline);
            cl.SetGraphicsRoot32BitConstants(0, 6, constants.as_ptr().cast(), 0);
            cl.SetGraphicsRootDescriptorTable(1, pass.views.GetGPUDescriptorHandleForHeapStart());
            cl.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
            cl.DrawInstanced(4, 1, 0, 0);
            cl.ResourceBarrier(&[transition(back, D3D12_RESOURCE_STATE_RENDER_TARGET, D3D12_RESOURCE_STATE_COPY_DEST)]);
        }
        self.cost += began.elapsed();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_card_is_painted_see_through_with_text() {
        let mut history = VecDeque::new();
        for i in 0..200 {
            history.push_back(if i % 10 == 0 { 90.0 } else { 55.0 });
        }
        let st = Stats { ms: 58.0, worst: 90.0, cpu: 44.0, gpu: 30.0, draws: 5200.0, vram: (7.0, 22.8), ram: 22.0, ..Default::default() };
        for full in [false, true] {
            let (w, h, pitch, bytes) = paint(&st, &history, full, 1.0, |w| (w * 4).next_multiple_of(256));
            assert_eq!(bytes.len(), pitch * h as usize);
            let at = |x: u32, y: u32| &bytes[y as usize * pitch + x as usize * 4..][..4];
            // the corner is outside the rounded card
            assert_eq!(at(0, 0)[3], 0);
            // the middle of the card is see-through dark
            let mid = at(w / 2, h - 6);
            assert!(mid[3] > 60 && mid[3] < 200, "alpha {}", mid[3]);
            // some text was drawn (bright, opaque pixels in the FPS number)
            let lit = (0..40).flat_map(|y| (14..90).map(move |x| (x, y))).filter(|&(x, y)| at(x, y)[3] > 230).count();
            assert!(lit > 50, "text pixels {lit}");
            assert_eq!(full, h > (CARD_H as u32) + 10);
        }
    }

    #[test]
    fn counters_that_went_back_to_nothing_count_from_there() {
        let (mut a, mut b) = (Counters::default(), Counters::default());
        (a.0[0], b.0[0]) = (10, 30);
        (a.0[1], b.0[1]) = (50, 20);
        let c = b.since(a, 10.0);
        assert_eq!((c[0], c[1]), (2.0, 2.0));
    }
}
