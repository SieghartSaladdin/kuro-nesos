//! Rendering: D3D11 device -> DXGI composition swap chain -> DirectComposition visual,
//! drawn with Direct2D + DirectWrite. All drawing coordinates are logical (96-DPI) units.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::mem::ManuallyDrop;

use windows::core::{w, Interface, Result};
use windows::Foundation::Numerics::Matrix3x2;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::*;
use windows::Win32::Graphics::Direct2D::*;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::DirectComposition::*;
use windows::Win32::Graphics::DirectWrite::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::Graphics::Imaging::*;
use windows::Win32::System::Com::*;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Align {
    Left,
    Center,
    Right,
}

pub fn col(hex: u32, a: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a,
    }
}

pub fn pt(x: f32, y: f32) -> D2D_POINT_2F {
    D2D_POINT_2F { x, y }
}

pub fn rc(x: f32, y: f32, w: f32, h: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: x, top: y, right: x + w, bottom: y + h }
}

fn identity() -> Matrix3x2 {
    Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 }
}

type FormatKey = (u32, u32, Align, bool);

/// Largest album-art edge kept in memory (pixels).
const ART_MAX: u32 = 160;

pub struct Gfx {
    factory: ID2D1Factory1,
    dwrite: IDWriteFactory,
    ctx: ID2D1DeviceContext,
    swap: IDXGISwapChain1,
    dcomp: IDCompositionDevice,
    _target: IDCompositionTarget,
    _visual: IDCompositionVisual,
    wic: IWICImagingFactory,
    brush: ID2D1SolidColorBrush,
    round: ID2D1StrokeStyle1,
    dashed: ID2D1StrokeStyle1,
    formats: RefCell<HashMap<FormatKey, IDWriteTextFormat>>,
    wrap_formats: RefCell<HashMap<(u32, u32), IDWriteTextFormat>>,
    pub scale: f32,
    /// Global opacity multiplier used for cross-fading whole views.
    pub alpha: Cell<f32>,
    errors: Cell<u32>,
}

impl Gfx {
    pub fn new(hwnd: HWND, px_w: u32, px_h: u32, scale: f32) -> Result<Self> {
        unsafe {
            let mut device: Option<ID3D11Device> = None;
            let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
            if D3D11CreateDevice(None, D3D_DRIVER_TYPE_HARDWARE, HMODULE::default(), flags, None, D3D11_SDK_VERSION, Some(&mut device), None, None).is_err() {
                D3D11CreateDevice(None, D3D_DRIVER_TYPE_WARP, HMODULE::default(), flags, None, D3D11_SDK_VERSION, Some(&mut device), None, None)?;
            }
            let d3d = device.unwrap();
            let dxgi_device: IDXGIDevice = d3d.cast()?;

            let options = D2D1_FACTORY_OPTIONS::default();
            let factory: ID2D1Factory1 = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, Some(&options))?;
            let d2d_device = factory.CreateDevice(&dxgi_device)?;
            let ctx = d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE)?;
            ctx.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);

            let adapter = dxgi_device.GetAdapter()?;
            let dxgi_factory: IDXGIFactory2 = adapter.GetParent()?;
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: px_w,
                Height: px_h,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                Stereo: FALSE,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                Scaling: DXGI_SCALING_STRETCH,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
                Flags: 0,
            };
            let swap = dxgi_factory.CreateSwapChainForComposition(&d3d, &desc, None)?;

            let dcomp: IDCompositionDevice = DCompositionCreateDevice(&dxgi_device)?;
            let target = dcomp.CreateTargetForHwnd(hwnd, true)?;
            let visual = dcomp.CreateVisual()?;
            visual.SetContent(&swap)?;
            target.SetRoot(&visual)?;
            dcomp.Commit()?;

            let surface: IDXGISurface = swap.GetBuffer(0)?;
            let props = D2D1_BITMAP_PROPERTIES1 {
                pixelFormat: D2D1_PIXEL_FORMAT { format: DXGI_FORMAT_B8G8R8A8_UNORM, alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED },
                dpiX: 96.0,
                dpiY: 96.0,
                bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                colorContext: ManuallyDrop::new(None),
            };
            let bitmap = ctx.CreateBitmapFromDxgiSurface(&surface, Some(&props))?;
            ctx.SetTarget(&bitmap);

            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let wic: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let brush = ctx.CreateSolidColorBrush(&col(0xffffff, 1.0), None)?;

            let mut sp = D2D1_STROKE_STYLE_PROPERTIES1 {
                startCap: D2D1_CAP_STYLE_ROUND,
                endCap: D2D1_CAP_STYLE_ROUND,
                dashCap: D2D1_CAP_STYLE_ROUND,
                lineJoin: D2D1_LINE_JOIN_ROUND,
                miterLimit: 10.0,
                dashStyle: D2D1_DASH_STYLE_SOLID,
                dashOffset: 0.0,
                transformType: D2D1_STROKE_TRANSFORM_TYPE_NORMAL,
            };
            let round = factory.CreateStrokeStyle(&sp, None)?;
            sp.dashStyle = D2D1_DASH_STYLE_CUSTOM;
            sp.dashCap = D2D1_CAP_STYLE_FLAT;
            sp.startCap = D2D1_CAP_STYLE_FLAT;
            sp.endCap = D2D1_CAP_STYLE_FLAT;
            let dashed = factory.CreateStrokeStyle(&sp, Some(&[4.0, 3.0]))?;

            Ok(Gfx {
                factory,
                dwrite,
                ctx,
                swap,
                dcomp,
                _target: target,
                _visual: visual,
                wic,
                brush,
                round,
                dashed,
                formats: RefCell::new(HashMap::new()),
                wrap_formats: RefCell::new(HashMap::new()),
                scale,
                alpha: Cell::new(1.0),
                errors: Cell::new(0),
            })
        }
    }

    fn rt(&self) -> &ID2D1RenderTarget {
        &self.ctx
    }

    pub fn begin(&self) {
        unsafe {
            self.rt().BeginDraw();
            self.rt().Clear(Some(&col(0, 0.0)));
        }
        self.set_offset(0.0, 0.0);
    }

    /// Finishes the frame. Returns false when the device was lost (driver reset, GPU switch
    /// on hybrid-graphics laptops...) and the renderer has to be recreated.
    pub fn end(&self) -> bool {
        unsafe {
            let mut ok = true;
            if let Err(e) = self.rt().EndDraw(None, None) {
                self.report("EndDraw", &e);
                ok = false;
            }
            let hr = self.swap.Present(1, DXGI_PRESENT(0));
            if hr.is_err() {
                self.report("Present", &windows::core::Error::from(hr));
                ok = false;
            }
            let _ = self.dcomp.Commit();
            ok
        }
    }

    fn report(&self, what: &str, e: &windows::core::Error) {
        let n = self.errors.get();
        if n < 20 {
            self.errors.set(n + 1);
            crate::sys::log(&format!("render: {} failed: {:?}", what, e));
        }
    }

    /// Logical translation applied on top of the DPI scale.
    pub fn set_offset(&self, x: f32, y: f32) {
        let s = self.scale;
        let m = Matrix3x2 { M11: s, M12: 0.0, M21: 0.0, M22: s, M31: x * s, M32: y * s };
        unsafe { self.rt().SetTransform(&m) };
    }

    /// Logical transform (rotation etc.) combined with the DPI scale.
    pub fn set_transform(&self, m: Matrix3x2) {
        let s = self.scale;
        let sm = Matrix3x2 { M11: m.M11 * s, M12: m.M12 * s, M21: m.M21 * s, M22: m.M22 * s, M31: m.M31 * s, M32: m.M32 * s };
        unsafe { self.rt().SetTransform(&sm) };
    }

    fn set(&self, hex: u32, a: f32) -> &ID2D1SolidColorBrush {
        unsafe { self.brush.SetColor(&col(hex, a * self.alpha.get())) };
        &self.brush
    }

    // ------------------------------------------------------------ primitives

    pub fn fill_rrect(&self, x: f32, y: f32, w: f32, h: f32, r: f32, hex: u32, a: f32) {
        let rr = D2D1_ROUNDED_RECT { rect: rc(x, y, w, h), radiusX: r, radiusY: r };
        unsafe { self.rt().FillRoundedRectangle(&rr, self.set(hex, a)) };
    }

    pub fn stroke_rrect(&self, x: f32, y: f32, w: f32, h: f32, r: f32, hex: u32, a: f32, width: f32, dashed: bool) {
        let rr = D2D1_ROUNDED_RECT { rect: rc(x, y, w, h), radiusX: r, radiusY: r };
        unsafe {
            let b = self.set(hex, a);
            if dashed {
                self.rt().DrawRoundedRectangle(&rr, b, width, &self.dashed);
            } else {
                self.rt().DrawRoundedRectangle(&rr, b, width, &self.round);
            }
        }
    }

    pub fn fill_circle(&self, cx: f32, cy: f32, r: f32, hex: u32, a: f32) {
        self.fill_ellipse(cx, cy, r, r, hex, a);
    }

    pub fn fill_ellipse(&self, cx: f32, cy: f32, rx: f32, ry: f32, hex: u32, a: f32) {
        let e = D2D1_ELLIPSE { point: pt(cx, cy), radiusX: rx, radiusY: ry };
        unsafe { self.rt().FillEllipse(&e, self.set(hex, a)) };
    }

    pub fn stroke_circle(&self, cx: f32, cy: f32, r: f32, hex: u32, a: f32, width: f32) {
        let e = D2D1_ELLIPSE { point: pt(cx, cy), radiusX: r, radiusY: r };
        unsafe { self.rt().DrawEllipse(&e, self.set(hex, a), width, &self.round) };
    }

    pub fn line(&self, x1: f32, y1: f32, x2: f32, y2: f32, hex: u32, a: f32, width: f32) {
        unsafe { self.rt().DrawLine(pt(x1, y1), pt(x2, y2), self.set(hex, a), width, &self.round) };
    }

    pub fn dashed_line(&self, x1: f32, y1: f32, x2: f32, y2: f32, hex: u32, a: f32, width: f32) {
        unsafe { self.rt().DrawLine(pt(x1, y1), pt(x2, y2), self.set(hex, a), width, &self.dashed) };
    }

    fn path(&self) -> Option<(ID2D1PathGeometry1, ID2D1GeometrySink)> {
        unsafe {
            let g = self.factory.CreatePathGeometry().ok()?;
            let sink = g.Open().ok()?;
            Some((g, sink))
        }
    }

    /// Stroked arc, angles in degrees (0 = +x, clockwise on screen).
    pub fn arc(&self, cx: f32, cy: f32, r: f32, start: f32, sweep: f32, hex: u32, a: f32, width: f32) {
        if sweep.abs() < 0.5 {
            return;
        }
        if sweep >= 359.5 {
            self.stroke_circle(cx, cy, r, hex, a, width);
            return;
        }
        let (a0, a1) = (start.to_radians(), (start + sweep).to_radians());
        let Some((g, sink)) = self.path() else { return };
        unsafe {
            sink.BeginFigure(pt(cx + r * a0.cos(), cy + r * a0.sin()), D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: pt(cx + r * a1.cos(), cy + r * a1.sin()),
                size: D2D_SIZE_F { width: r, height: r },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: if sweep > 180.0 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            let _ = sink.Close();
            self.rt().DrawGeometry(&g, self.set(hex, a), width, &self.round);
        }
    }

    /// Filled pie slice (used for tiny progress indicators).
    pub fn pie(&self, cx: f32, cy: f32, r: f32, start: f32, sweep: f32, hex: u32, a: f32) {
        if sweep >= 359.5 {
            self.fill_circle(cx, cy, r, hex, a);
            return;
        }
        if sweep < 0.5 {
            return;
        }
        let (a0, a1) = (start.to_radians(), (start + sweep).to_radians());
        let Some((g, sink)) = self.path() else { return };
        unsafe {
            sink.BeginFigure(pt(cx, cy), D2D1_FIGURE_BEGIN_FILLED);
            sink.AddLine(pt(cx + r * a0.cos(), cy + r * a0.sin()));
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: pt(cx + r * a1.cos(), cy + r * a1.sin()),
                size: D2D_SIZE_F { width: r, height: r },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: if sweep > 180.0 { D2D1_ARC_SIZE_LARGE } else { D2D1_ARC_SIZE_SMALL },
            });
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            let _ = sink.Close();
            self.rt().FillGeometry(&g, self.set(hex, a), None);
        }
    }

    pub fn polygon(&self, pts: &[(f32, f32)], hex: u32, a: f32) {
        if pts.len() < 3 {
            return;
        }
        let Some((g, sink)) = self.path() else { return };
        unsafe {
            sink.BeginFigure(pt(pts[0].0, pts[0].1), D2D1_FIGURE_BEGIN_FILLED);
            let rest: Vec<D2D_POINT_2F> = pts[1..].iter().map(|p| pt(p.0, p.1)).collect();
            sink.AddLines(&rest);
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            let _ = sink.Close();
            self.rt().FillGeometry(&g, self.set(hex, a), None);
            self.rt().DrawGeometry(&g, self.set(hex, a), 1.2, &self.round);
        }
    }

    /// Stroked open polyline with round joins (happy eyes, chevrons...).
    pub fn polyline(&self, pts: &[(f32, f32)], hex: u32, a: f32, width: f32) {
        if pts.len() < 2 {
            return;
        }
        let Some((g, sink)) = self.path() else { return };
        unsafe {
            sink.BeginFigure(pt(pts[0].0, pts[0].1), D2D1_FIGURE_BEGIN_HOLLOW);
            let rest: Vec<D2D_POINT_2F> = pts[1..].iter().map(|p| pt(p.0, p.1)).collect();
            sink.AddLines(&rest);
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            let _ = sink.Close();
            self.rt().DrawGeometry(&g, self.set(hex, a), width, &self.round);
        }
    }

    /// The notch silhouette: flat top with small concave "shoulders", rounded bottom corners.
    /// `cx` is the horizontal centre; the shape hangs from y = 0.
    pub fn notch_geometry(&self, cx: f32, w: f32, h: f32, r: f32, shoulder: f32) -> Option<ID2D1PathGeometry1> {
        let x0 = cx - w / 2.0;
        let x1 = cx + w / 2.0;
        let r = r.min(h / 2.0).min(w / 2.0).max(0.0);
        let s = shoulder.min(h - r).max(0.0);
        let (g, sink) = self.path()?;
        unsafe {
            sink.BeginFigure(pt(x0 - s, 0.0), D2D1_FIGURE_BEGIN_FILLED);
            if s > 0.1 {
                sink.AddArc(&arc_seg(x0, s, s, D2D1_SWEEP_DIRECTION_CLOCKWISE));
            }
            sink.AddLine(pt(x0, h - r));
            sink.AddArc(&arc_seg(x0 + r, h, r, D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE));
            sink.AddLine(pt(x1 - r, h));
            sink.AddArc(&arc_seg(x1, h - r, r, D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE));
            sink.AddLine(pt(x1, s));
            if s > 0.1 {
                sink.AddArc(&arc_seg(x1 + s, 0.0, s, D2D1_SWEEP_DIRECTION_CLOCKWISE));
            }
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close().ok()?;
        }
        Some(g)
    }

    /// Notch docked against a vertical screen edge: flat on the edge side (with concave
    /// shoulders above and below), rounded on the inner side. `edge_x` is the edge's x
    /// coordinate (0 for the left edge, the window width for the right edge).
    pub fn side_geometry(&self, edge_x: f32, y0: f32, w: f32, h: f32, r: f32, shoulder: f32, left: bool) -> Option<ID2D1PathGeometry1> {
        let y1 = y0 + h;
        let r = r.min(h / 2.0).min(w / 2.0).max(0.0);
        let s = shoulder.min(w - r).max(0.0);
        let cw = D2D1_SWEEP_DIRECTION_CLOCKWISE;
        let ccw = D2D1_SWEEP_DIRECTION_COUNTER_CLOCKWISE;
        let (g, sink) = self.path()?;
        unsafe {
            if left {
                let x1 = edge_x + w;
                sink.BeginFigure(pt(edge_x, y0 - s), D2D1_FIGURE_BEGIN_FILLED);
                if s > 0.1 {
                    sink.AddArc(&arc_seg(edge_x + s, y0, s, ccw));
                }
                sink.AddLine(pt(x1 - r, y0));
                sink.AddArc(&arc_seg(x1, y0 + r, r, cw));
                sink.AddLine(pt(x1, y1 - r));
                sink.AddArc(&arc_seg(x1 - r, y1, r, cw));
                sink.AddLine(pt(edge_x + s, y1));
                if s > 0.1 {
                    sink.AddArc(&arc_seg(edge_x, y1 + s, s, ccw));
                }
            } else {
                let x0 = edge_x - w;
                sink.BeginFigure(pt(edge_x, y0 - s), D2D1_FIGURE_BEGIN_FILLED);
                if s > 0.1 {
                    sink.AddArc(&arc_seg(edge_x - s, y0, s, cw));
                }
                sink.AddLine(pt(x0 + r, y0));
                sink.AddArc(&arc_seg(x0, y0 + r, r, ccw));
                sink.AddLine(pt(x0, y1 - r));
                sink.AddArc(&arc_seg(x0 + r, y1, r, ccw));
                sink.AddLine(pt(edge_x - s, y1));
                if s > 0.1 {
                    sink.AddArc(&arc_seg(edge_x, y1 + s, s, cw));
                }
            }
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close().ok()?;
        }
        Some(g)
    }

    pub fn fill_geometry(&self, g: &ID2D1PathGeometry1, hex: u32, a: f32) {
        unsafe { self.rt().FillGeometry(g, self.set(hex, a), None) };
    }

    // ------------------------------------------------------------ gradients & glow

    fn stops(&self, stops: &[(f32, u32, f32)]) -> Option<ID2D1GradientStopCollection> {
        let st: Vec<D2D1_GRADIENT_STOP> = stops.iter().map(|&(p, hex, a)| D2D1_GRADIENT_STOP { position: p, color: col(hex, a * self.alpha.get()) }).collect();
        unsafe { self.rt().CreateGradientStopCollection(&st, D2D1_GAMMA_2_2, D2D1_EXTEND_MODE_CLAMP).ok() }
    }

    pub fn linear(&self, x0: f32, y0: f32, x1: f32, y1: f32, stops: &[(f32, u32, f32)]) -> Option<ID2D1Brush> {
        let coll = self.stops(stops)?;
        let props = D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES { startPoint: pt(x0, y0), endPoint: pt(x1, y1) };
        unsafe { self.rt().CreateLinearGradientBrush(&props, None, &coll).ok().and_then(|b| b.cast().ok()) }
    }

    pub fn radial(&self, cx: f32, cy: f32, rx: f32, ry: f32, stops: &[(f32, u32, f32)]) -> Option<ID2D1Brush> {
        let coll = self.stops(stops)?;
        let props = D2D1_RADIAL_GRADIENT_BRUSH_PROPERTIES { center: pt(cx, cy), gradientOriginOffset: pt(0.0, 0.0), radiusX: rx, radiusY: ry };
        unsafe { self.rt().CreateRadialGradientBrush(&props, None, &coll).ok().and_then(|b| b.cast().ok()) }
    }

    pub fn fill_rrect_brush(&self, x: f32, y: f32, w: f32, h: f32, r: f32, brush: &ID2D1Brush) {
        let rr = D2D1_ROUNDED_RECT { rect: rc(x, y, w, h), radiusX: r, radiusY: r };
        unsafe { self.rt().FillRoundedRectangle(&rr, brush) };
    }

    /// Soft glow: radial gradient from `a` at the centre to transparent at the rim.
    pub fn glow(&self, cx: f32, cy: f32, rx: f32, ry: f32, hex: u32, a: f32) {
        if let Some(b) = self.radial(cx, cy, rx, ry, &[(0.0, hex, a), (0.45, hex, a * 0.45), (1.0, hex, 0.0)]) {
            let e = D2D1_ELLIPSE { point: pt(cx, cy), radiusX: rx, radiusY: ry };
            unsafe { self.rt().FillEllipse(&e, &b) };
        }
    }

    /// Restricts drawing to a geometry until `pop_clip`.
    pub fn push_clip(&self, g: &ID2D1PathGeometry1) {
        unsafe {
            let mut params = D2D1_LAYER_PARAMETERS1 {
                contentBounds: D2D_RECT_F { left: -1e6, top: -1e6, right: 1e6, bottom: 1e6 },
                geometricMask: ManuallyDrop::new(g.cast::<ID2D1Geometry>().ok()),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: identity(),
                opacity: 1.0,
                opacityBrush: ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS1_NONE,
            };
            self.ctx.PushLayer(&params, None);
            ManuallyDrop::drop(&mut params.geometricMask);
        }
    }

    /// Draws everything until `pop_clip` at `opacity` (used to fade the whole notch).
    pub fn push_fade(&self, opacity: f32) {
        unsafe {
            let params = D2D1_LAYER_PARAMETERS1 {
                contentBounds: D2D_RECT_F { left: -1e6, top: -1e6, right: 1e6, bottom: 1e6 },
                geometricMask: ManuallyDrop::new(None),
                maskAntialiasMode: D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
                maskTransform: identity(),
                opacity,
                opacityBrush: ManuallyDrop::new(None),
                layerOptions: D2D1_LAYER_OPTIONS1_NONE,
            };
            self.ctx.PushLayer(&params, None);
        }
    }

    pub fn pop_clip(&self) {
        unsafe { self.rt().PopLayer() };
    }

    // ------------------------------------------------------------ text

    fn format(&self, size: f32, weight: u32, align: Align, icon: bool) -> IDWriteTextFormat {
        let key = ((size * 10.0) as u32, weight, align, icon);
        if let Some(f) = self.formats.borrow().get(&key) {
            return f.clone();
        }
        unsafe {
            let family = if icon { w!("Segoe Fluent Icons") } else { w!("Segoe UI Variable Text") };
            let f = self
                .dwrite
                .CreateTextFormat(family, None, DWRITE_FONT_WEIGHT(weight as i32), DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("en-us"))
                .unwrap();
            let _ = f.SetTextAlignment(match align {
                Align::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
                Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
                Align::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
            });
            let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
            let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP);
            if !icon {
                let trim = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER, delimiter: 0, delimiterCount: 0 };
                if let Ok(sign) = self.dwrite.CreateEllipsisTrimmingSign(&f) {
                    let _ = f.SetTrimming(&trim, &sign);
                }
            }
            self.formats.borrow_mut().insert(key, f.clone());
            f
        }
    }

    /// Draws single-line text vertically centred on `cy`. For Left the text starts at `x`,
    /// for Center it is centred on `x`, for Right it ends at `x`. `max_w` bounds the run
    /// (ellipsis trimming).
    pub fn text(&self, s: &str, x: f32, cy: f32, size: f32, weight: u32, align: Align, hex: u32, a: f32, max_w: f32) {
        let half = size * 1.2;
        let r = match align {
            Align::Left => D2D_RECT_F { left: x, top: cy - half, right: x + max_w, bottom: cy + half },
            Align::Center => D2D_RECT_F { left: x - max_w / 2.0, top: cy - half, right: x + max_w / 2.0, bottom: cy + half },
            Align::Right => D2D_RECT_F { left: x - max_w, top: cy - half, right: x, bottom: cy + half },
        };
        let wide: Vec<u16> = s.encode_utf16().collect();
        let f = self.format(size, weight, align, false);
        unsafe { self.rt().DrawText(&wide, &f, &r, self.set(hex, a), D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL) };
    }

    /// Word-wrapping, top-aligned format (cached) for multi-line text.
    fn wrap_format(&self, size: f32, weight: u32) -> IDWriteTextFormat {
        let key = ((size * 10.0) as u32, weight);
        if let Some(f) = self.wrap_formats.borrow().get(&key) {
            return f.clone();
        }
        unsafe {
            let f = self
                .dwrite
                .CreateTextFormat(w!("Segoe UI Variable Text"), None, DWRITE_FONT_WEIGHT(weight as i32), DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, size, w!("en-us"))
                .unwrap();
            let _ = f.SetWordWrapping(DWRITE_WORD_WRAPPING_WRAP);
            let _ = f.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
            let trim = DWRITE_TRIMMING { granularity: DWRITE_TRIMMING_GRANULARITY_WORD, delimiter: 0, delimiterCount: 0 };
            if let Ok(sign) = self.dwrite.CreateEllipsisTrimmingSign(&f) {
                let _ = f.SetTrimming(&trim, &sign);
            }
            self.wrap_formats.borrow_mut().insert(key, f.clone());
            f
        }
    }

    /// Wrapped text inside (x, y, w, h); lines beyond the box end with an ellipsis.
    pub fn text_box(&self, s: &str, x: f32, y: f32, w: f32, h: f32, size: f32, weight: u32, hex: u32, a: f32) {
        let wide: Vec<u16> = s.encode_utf16().collect();
        let f = self.wrap_format(size, weight);
        let r = D2D_RECT_F { left: x, top: y, right: x + w, bottom: y + h };
        unsafe { self.rt().DrawText(&wide, &f, &r, self.set(hex, a), D2D1_DRAW_TEXT_OPTIONS_CLIP, DWRITE_MEASURING_MODE_NATURAL) };
    }

    /// Height of `s` wrapped to `width` (same format as `text_box`).
    pub fn text_height(&self, s: &str, width: f32, size: f32, weight: u32) -> f32 {
        let wide: Vec<u16> = s.encode_utf16().collect();
        let f = self.wrap_format(size, weight);
        unsafe {
            if let Ok(layout) = self.dwrite.CreateTextLayout(&wide, &f, width, 2000.0) {
                let mut m = DWRITE_TEXT_METRICS::default();
                if layout.GetMetrics(&mut m).is_ok() {
                    return m.height;
                }
            }
        }
        size * 1.4
    }

    pub fn measure(&self, s: &str, size: f32, weight: u32) -> f32 {
        let wide: Vec<u16> = s.encode_utf16().collect();
        let f = self.format(size, weight, Align::Left, false);
        unsafe {
            if let Ok(layout) = self.dwrite.CreateTextLayout(&wide, &f, 4000.0, 200.0) {
                let mut m = DWRITE_TEXT_METRICS::default();
                if layout.GetMetrics(&mut m).is_ok() {
                    return m.widthIncludingTrailingWhitespace;
                }
            }
        }
        s.chars().count() as f32 * size * 0.55
    }

    /// Segoe Fluent Icons glyph centred on (cx, cy).
    pub fn icon(&self, glyph: u16, cx: f32, cy: f32, size: f32, hex: u32, a: f32) {
        let r = D2D_RECT_F { left: cx - size * 2.0, top: cy - size, right: cx + size * 2.0, bottom: cy + size };
        let f = self.format(size, 400, Align::Center, true);
        unsafe { self.rt().DrawText(&[glyph], &f, &r, self.set(hex, a), D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL) };
    }

    // ------------------------------------------------------------ bitmaps

    pub fn bitmap_from_bytes(&self, bytes: &[u8]) -> Option<ID2D1Bitmap> {
        unsafe {
            let stream = self.wic.CreateStream().ok()?;
            stream.InitializeFromMemory(bytes).ok()?;
            let dec = self.wic.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
            let frame = dec.GetFrame(0).ok()?;
            // album art is shown at most ~62 px (93 px at 150 %): decode it small
            let (mut w, mut h) = (0u32, 0u32);
            frame.GetSize(&mut w, &mut h).ok()?;
            let source: IWICBitmapSource = if w.max(h) > ART_MAX {
                let k = ART_MAX as f32 / w.max(h) as f32;
                let scaler = self.wic.CreateBitmapScaler().ok()?;
                scaler.Initialize(&frame, ((w as f32 * k) as u32).max(1), ((h as f32 * k) as u32).max(1), WICBitmapInterpolationModeFant).ok()?;
                scaler.cast().ok()?
            } else {
                frame.cast().ok()?
            };
            let conv = self.wic.CreateFormatConverter().ok()?;
            conv.Initialize(&source, &GUID_WICPixelFormat32bppPBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeMedianCut).ok()?;
            self.rt().CreateBitmapFromWicBitmap(&conv, None).ok()
        }
    }

    /// Draws a bitmap scaled to fill (x, y, w, h) with rounded corners ("cover" crop).
    pub fn bitmap_rounded(&self, bmp: &ID2D1Bitmap, x: f32, y: f32, w: f32, h: f32, r: f32) {
        unsafe {
            let size = bmp.GetSize();
            if size.width <= 0.0 || size.height <= 0.0 {
                return;
            }
            let k = (w / size.width).max(h / size.height);
            let dx = x + (w - size.width * k) / 2.0;
            let dy = y + (h - size.height * k) / 2.0;
            let Ok(brush) = self.rt().CreateBitmapBrush(bmp, None, None) else { return };
            brush.SetTransform(&Matrix3x2 { M11: k, M12: 0.0, M21: 0.0, M22: k, M31: dx, M32: dy });
            brush.SetOpacity(self.alpha.get());
            let rr = D2D1_ROUNDED_RECT { rect: rc(x, y, w, h), radiusX: r, radiusY: r };
            self.rt().FillRoundedRectangle(&rr, &brush);
        }
    }
}

fn arc_seg(x: f32, y: f32, r: f32, dir: D2D1_SWEEP_DIRECTION) -> D2D1_ARC_SEGMENT {
    D2D1_ARC_SEGMENT { point: pt(x, y), size: D2D_SIZE_F { width: r, height: r }, rotationAngle: 0.0, sweepDirection: dir, arcSize: D2D1_ARC_SIZE_SMALL }
}

/// Rotation (degrees) around (cx, cy), in logical units.
pub fn rotation(deg: f32, cx: f32, cy: f32) -> Matrix3x2 {
    let (s, c) = deg.to_radians().sin_cos();
    Matrix3x2 { M11: c, M12: s, M21: -s, M22: c, M31: cx - c * cx + s * cy, M32: cy - s * cx - c * cy }
}
