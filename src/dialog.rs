//! `enclave --confirm`: the window that asks, and nothing else.
//!
//! One process per question, started by the server. The question arrives as one
//! JSON line on stdin, the answer leaves as one JSON line on stdout, and then
//! this process is done. It never touches the app socket: an approval can only
//! come down that pipe, from a click in this window.
//!
//! A dialog of its own is what lets the server be headless. The alternative —
//! the server owning an event loop so it can open a viewport — costs it a root
//! window it cannot hide on Wayland, for a window that is on screen for seconds
//! a day.
//!
//! Saying nothing is a refusal, so every way out of here that is not a click on
//! Approve writes a denial: Escape, the window's close button, the countdown
//! running out, and a crash — that last one by the server hearing nothing.

use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::confirm::{Answer, Decision, Request};

/// Reads the question, asks it, and prints the answer.
pub fn run() -> eframe::Result {
    let Some(request) = read_request() else {
        // Nothing legible to ask about. Silence is a refusal, so the server
        // does the right thing with a child that says nothing at all.
        return Ok(());
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Enclave")
            .with_inner_size([460.0, 220.0])
            .with_resizable(false)
            .with_decorations(true)
            .with_always_on_top(),
        ..Default::default()
    };

    // Whether the one line has been printed, so the fallback below cannot print
    // a second one.
    let spoken = Arc::new(AtomicBool::new(false));
    let app_spoken = spoken.clone();
    let result = eframe::run_native(
        "enclave",
        options,
        Box::new(move |cc| {
            enclave_ui::fonts::install(&cc.egui_ctx);
            cc.egui_ctx
                .set_visuals(enclave_ui::theme::visuals(&enclave_ui::theme::load()));
            Ok(Box::new(Dialog::new(request, app_spoken)))
        }),
    );

    // The window went away without answering — the compositor closed it, or
    // eframe could not start at all. Either way the answer is no.
    if !spoken.load(Ordering::SeqCst) {
        say(&Answer::deny(), &spoken);
    }
    result
}

/// The question, from the one line the server wrote to our stdin.
fn read_request() -> Option<Request> {
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).ok()?;
    Request::parse(&line)
}

/// Prints the decision, once. Stdout carries nothing else, ever.
fn say(answer: &Answer, spoken: &AtomicBool) {
    if spoken.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{}", answer.encode());
    let _ = out.flush();
}

struct Dialog {
    request: Request,
    /// Ticks down from `request.seconds`; reaching zero is a refusal.
    started: Instant,
    always: bool,
    spoken: Arc<AtomicBool>,
}

impl Dialog {
    fn new(request: Request, spoken: Arc<AtomicBool>) -> Dialog {
        Dialog {
            request,
            started: Instant::now(),
            always: false,
            spoken,
        }
    }

    fn left(&self) -> u64 {
        self.request
            .seconds
            .saturating_sub(self.started.elapsed().as_secs())
    }

    fn answer(&self, ctx: &egui::Context, decision: Decision) {
        say(
            &Answer {
                decision,
                always_allow: self.always && decision == Decision::Approve,
            },
            &self.spoken,
        );
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

impl eframe::App for Dialog {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let left = self.left();
        let mut decision: Option<Decision> = None;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Allow this?").strong());
            ui.add_space(6.0);
            ui.label(egui::RichText::new(&self.request.sentence).size(15.0));
            if !self.request.args.is_empty() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&self.request.args).weak().size(11.0));
            }
            ui.add_space(10.0);
            ui.checkbox(&mut self.always, &self.request.always_label);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Deny").clicked() {
                    decision = Some(Decision::Deny);
                }
                if ui.button("Approve").clicked() {
                    decision = Some(Decision::Approve);
                }
                ui.add_space(8.0);
                ui.label(egui::RichText::new(format!("Denying in {left} s")).weak());
            });
        });

        // Closing the window, or Escape, is a refusal. There is no key that
        // approves: a window that takes focus while someone is typing must not
        // be able to collect a yes by accident.
        if left == 0
            || ctx.input(|i| i.viewport().close_requested())
            || ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            decision = Some(Decision::Deny);
        }

        if let Some(decision) = decision {
            self.answer(ctx, decision);
        }
        // Keep the countdown moving.
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}
