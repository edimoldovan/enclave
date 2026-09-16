//! Offscreen probe (temporary): the real `web::Body` in a real eframe/X11
//! window that is never mapped, resized the way a user resizes it — the X
//! window itself is resized, winit sees ConfigureNotify, egui's panels follow,
//! and `detail_view`'s measurement of the pane changes.

use eframe::egui;
use post_ui::web::{self, Body, Placed, Px, Ready};

const HTML: &str = "<p>probe body</p>";

/// Resizes the app's own X window behind winit's back — which is what a window
/// manager does when the user drags an edge.
fn resize_x_window(xid: std::os::raw::c_ulong, w: u32, h: u32) {
    let xlib = x11_dl::xlib::Xlib::open().expect("xlib");
    unsafe {
        let display = (xlib.XOpenDisplay)(std::ptr::null());
        assert!(!display.is_null(), "no X display");
        (xlib.XResizeWindow)(display, xid, w, h);
        (xlib.XFlush)(display);
        (xlib.XCloseDisplay)(display);
    }
}

struct Probe {
    ready: Ready,
    body: Body,
    n: u32,
    key: String,
    xid: std::os::raw::c_ulong,
}

fn line(tag: &str, n: u32, at: Px, body: &Body, placed: Placed, panel: egui::Rect) {
    let applied = body
        .debug_applied()
        .map(|(x, y, w, h)| format!("{w}x{h} @ {x},{y}"))
        .unwrap_or_else(|| "none".into());
    let x = body
        .debug_geometry()
        .map(|(x, y, w, h)| format!("{w}x{h} @ {x},{y}"))
        .unwrap_or_else(|| "none".into());
    println!(
        "f{n:<4} {tag:<24} {placed:?} | panel {}x{} pt | egui asked {}x{} @ {},{} | gate \
         applied {applied} | X child {x}",
        panel.width() as i32,
        panel.height() as i32,
        at.w as i32,
        at.h as i32,
        at.x as i32,
        at.y as i32,
    );
}

impl eframe::App for Probe {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        web::pump();
        self.n += 1;
        let n = self.n;
        let ppp = ctx.pixels_per_point();

        match n {
            40 => {
                println!("f{n:<4} -- the window is dragged bigger: 1500x950 --");
                resize_x_window(self.xid, 1500, 950);
            }
            90 => {
                println!("f{n:<4} -- the window is dragged smaller: 700x500 --");
                resize_x_window(self.xid, 700, 500);
            }
            140 => {
                self.body.hide();
                println!("f{n:<4} -- back to the list (hide) --");
            }
            160 => {
                self.key = "acct/2".into();
                println!("f{n:<4} -- open another message (same window size) --");
            }
            _ => {}
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            // Stand-in for the subject/from header detail.rs draws.
            ui.add_space(70.0);
            ui.separator();
            let panel = ui.max_rect();
            let area = ui.available_rect_before_wrap();
            let at = Px {
                x: (area.min.x * ppp) as f64,
                y: (area.min.y * ppp) as f64,
                w: (area.width() * ppp) as f64,
                h: (area.height() * ppp) as f64,
            };
            if (140..160).contains(&n) {
                self.body.hide();
                ui.allocate_space(area.size());
                return;
            }
            let key = self.key.clone();
            let placed = self.body.show(frame, self.ready, &key, HTML, at);
            ui.allocate_space(area.size());

            match n {
                30 => line("A settled", n, at, &self.body, placed, panel),
                60 | 85 => line("B window grew", n, at, &self.body, placed, panel),
                110 | 135 => line("C window shrank", n, at, &self.body, placed, panel),
                165 | 195 => line("D another message", n, at, &self.body, placed, panel),
                _ => {}
            }
        });

        ctx.request_repaint();
        if n >= 200 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

fn main() -> eframe::Result {
    let ready = web::prepare();
    println!("ready = {ready:?}");
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_visible(false)
            .with_title("pane probe"),
        ..Default::default()
    };
    if ready.x11 {
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            builder.with_x11();
        }));
    }
    eframe::run_native(
        "pane-probe",
        options,
        Box::new(move |cc| {
            enclave::xerror::quarantine_foreign(winit_display(cc));
            let xid = winit_window(cc);
            println!("app X window = 0x{xid:x} (created, never mapped)");
            Ok(Box::new(Probe {
                ready,
                body: Body::new(Vec::new(), None, None),
                n: 0,
                key: "acct/1".into(),
                xid,
            }))
        }),
    )
}

fn winit_display(cc: &eframe::CreationContext<'_>) -> usize {
    use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};

    match cc.display_handle().map(|handle| handle.as_raw()) {
        Ok(RawDisplayHandle::Xlib(x11)) => {
            x11.display.map_or(0, |display| display.as_ptr() as usize)
        }
        _ => 0,
    }
}

fn winit_window(cc: &eframe::CreationContext<'_>) -> std::os::raw::c_ulong {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    match cc.window_handle().map(|handle| handle.as_raw()) {
        Ok(RawWindowHandle::Xlib(x11)) => x11.window,
        _ => 0,
    }
}
