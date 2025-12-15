#![cfg(not(target_arch = "wasm32"))]

use std::sync::mpsc::{self, Receiver};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use widgetry::{
    EventCtx, GfxCtx, HorizontalAlignment, Line, MultilineTextBox, Outcome, Panel, ScreenDims,
    Text, VerticalAlignment, Widget,
};

use crate::sandbox::SpeedSetting;

#[derive(Clone)]
enum Role {
    User,
    Assistant,
    System,
}

pub enum ChatCommand {
    Pause,
    Resume,
}

pub struct Chatbox {
    panel: Panel,
    messages: Vec<(Role, String)>,
    input_prefill: String,
    pending_rx: Option<Receiver<Result<String>>>,
    pending_command: Option<ChatCommand>,
    width_pct: usize,
    height_pct: usize,
}

impl Chatbox {
    pub fn new(ctx: &mut EventCtx) -> Chatbox {
        let mut cb = Chatbox {
            panel: Panel::empty(ctx),
            messages: vec![(Role::System, "Chatbox ready.".to_string())],
            input_prefill: "I want to evaluate how different ride-hailing vehicle quotas (from 1,000 to 10,000) affect road traffic congestion in Hong Kong.".to_string(),
            pending_rx: None,
            pending_command: None,
            width_pct: 35,
            height_pct: 35,
        };
        cb.rebuild_panel(ctx);
        cb
    }

    pub fn event(&mut self, ctx: &mut EventCtx) {
        // Check for inflight LLM response
        if let Some(rx) = &self.pending_rx {
            if let Ok(res) = rx.try_recv() {
                self.pending_rx = None;
                match res {
                    Ok(content) => {
                        self.messages.push((Role::Assistant, content.clone()));
                        self.pending_command = parse_command(&content);
                    }
                    Err(err) => {
                        self.messages.push((Role::System, format!("LLM error: {err:#}")));
                    }
                }
                self.rebuild_panel(ctx);
            }
        }

        // Keep local copy of input in sync
        if self.panel.maybe_find::<MultilineTextBox>("chat_input").is_some() {
            self.input_prefill = self
                .panel
                .find::<MultilineTextBox>("chat_input")
                .get_text();
        }

        match self.panel.event(ctx) {
            Outcome::Clicked(x) if x == "send" => {
                let input = self
                    .panel
                    .find::<MultilineTextBox>("chat_input")
                    .get_text();
                let trimmed = input.trim();
                if trimmed.is_empty() || self.pending_rx.is_some() {
                    return;
                }
                self.messages.push((Role::User, trimmed.to_string()));
                self.input_prefill.clear();
                self.rebuild_panel(ctx);
                self.start_request(trimmed.to_string());
            }
            Outcome::Clicked(x) if x == "smaller" => {
                // snapshot current text before rebuild
                self.input_prefill = self
                    .panel
                    .find::<MultilineTextBox>("chat_input")
                    .get_text();
                self.width_pct = self.width_pct.saturating_sub(5).max(15);
                self.height_pct = self.height_pct.saturating_sub(5).max(15);
                self.rebuild_panel(ctx);
            }
            Outcome::Clicked(x) if x == "larger" => {
                self.input_prefill = self
                    .panel
                    .find::<MultilineTextBox>("chat_input")
                    .get_text();
                self.width_pct = (self.width_pct + 5).min(50);
                self.height_pct = (self.height_pct + 5).min(60);
                self.rebuild_panel(ctx);
            }
            _ => {}
        }
    }

    pub fn draw(&self, g: &mut GfxCtx) {
        self.panel.draw(g);
    }

    pub fn recreate_panel(&mut self, ctx: &mut EventCtx) {
        self.rebuild_panel(ctx);
    }

    pub fn take_command(&mut self) -> Option<ChatCommand> {
        self.pending_command.take()
    }

    fn rebuild_panel(&mut self, ctx: &mut EventCtx) {
        let mut col = Vec::new();
        col.push(
            Widget::row(vec![
                Line("LLM Chat (Sylvia's Team)")
                    .small_heading()
                    .into_widget(ctx)
                    .margin_right(10),
                ctx.style()
                    .btn_plain
                    .text("-")
                    .build_widget(ctx, "smaller"),
                ctx.style()
                    .btn_plain
                    .text("+")
                    .build_widget(ctx, "larger")
                    .margin_left(4),
            ])
            .centered_vert(),
        );

        let recent = self
            .messages
            .iter()
            .rev()
            .take(6)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>();
        for (role, msg) in recent {
            let prefix = match role {
                Role::User => "You: ",
                Role::Assistant => "LLM: ",
                Role::System => "",
            };
            col.push(
                Text::from(Line(format!("{prefix}{msg}")))
                    .wrap_to_pct(ctx, (self.width_pct as f64 * 0.9).round() as usize)
                    .into_widget(ctx)
                    .margin_above(4),
            );
        }

        let win = ctx.canvas.get_window_dims();
        let panel_w_px = (self.width_pct as f64 / 100.0) * win.width;
        let panel_h_px = (self.height_pct as f64 / 100.0) * win.height;
        let input_dims = ScreenDims::new(
            (panel_w_px * 0.65).max(220.0),
            (panel_h_px * 0.30).max(90.0),
        );

        let row = Widget::row(vec![
            MultilineTextBox::widget(
                ctx,
                "chat_input",
                self.input_prefill.clone(),
                input_dims,
                false,
            )
                .margin_right(6),
            ctx.style()
                .btn_outline
                .text(if self.pending_rx.is_some() { "..." } else { "Send" })
                .build_widget(ctx, "send")
                .centered_vert(),
        ])
        .margin_above(6);
        col.push(row);

        self.panel = Panel::new_builder(Widget::col(col).padding(8).bg(ctx.style().panel_bg))
            .aligned_pair((
                HorizontalAlignment::Percent(0.02),
                VerticalAlignment::Percent(0.65),
            ))
            .exact_size_percent(self.width_pct, self.height_pct)
            .build_custom(ctx);
    }

    fn start_request(&mut self, user_msg: String) {
        let history = self.messages.clone();
        let (tx, rx) = mpsc::channel();
        self.pending_rx = Some(rx);
        std::thread::spawn(move || {
            let res = fetch_deepseek_reply(history, user_msg);
            let _ = tx.send(res);
        });
    }
}

fn parse_command(reply: &str) -> Option<ChatCommand> {
    let lower = reply.to_lowercase();
    if lower.contains("action: pause") || lower.trim() == "pause" || lower.contains("/pause") {
        Some(ChatCommand::Pause)
    } else if lower.contains("action: resume")
        || lower.trim() == "resume"
        || lower.contains("/resume")
        || lower.contains("/play")
    {
        Some(ChatCommand::Resume)
    } else {
        None
    }
}

#[derive(Serialize)]
struct DeepseekChatRequest {
    model: String,
    messages: Vec<DeepseekMessage>,
    temperature: f32,
}

#[derive(Serialize)]
struct DeepseekMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct DeepseekChatResponse {
    choices: Vec<DeepseekChoice>,
}

#[derive(Deserialize)]
struct DeepseekChoice {
    message: DeepseekMessageOut,
}

#[derive(Deserialize)]
struct DeepseekMessageOut {
    content: String,
}

fn fetch_deepseek_reply(history: Vec<(Role, String)>, user_msg: String) -> Result<String> {
    let api_key = std::env::var("DEEPSEEK_API_KEY")
        .map_err(|_| anyhow::anyhow!("Missing DEEPSEEK_API_KEY env var"))?;
    let base = std::env::var("DEEPSEEK_BASE_URL")
        .unwrap_or_else(|_| "https://api.deepseek.com/v1".to_string());
    let url = format!("{}/chat/completions", base.trim_end_matches('/'));

    let mut messages = Vec::new();
    messages.push(DeepseekMessage {
        role: "system".to_string(),
        content: "You are controlling a traffic simulation. You may include lines like \
ACTION: pause or ACTION: resume. Keep replies short."
            .to_string(),
    });
    for (role, content) in history.into_iter().rev().take(8).rev() {
        let r = match role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::System => "system",
        };
        messages.push(DeepseekMessage {
            role: r.to_string(),
            content,
        });
    }
    messages.push(DeepseekMessage {
        role: "user".to_string(),
        content: user_msg,
    });

    let req = DeepseekChatRequest {
        model: "deepseek-chat".to_string(),
        messages,
        temperature: 0.2,
    };

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(url)
        .bearer_auth(api_key)
        .json(&req)
        .send()?
        .error_for_status()?;
    let body: DeepseekChatResponse = resp.json()?;
    let content = body
        .choices
        .get(0)
        .map(|c| c.message.content.clone())
        .unwrap_or_else(|| "(empty reply)".to_string());
    Ok(content)
}

// Keep the compiler from warning about unused imports in some builds.
#[allow(dead_code)]
fn _default_resume_setting() -> SpeedSetting {
    SpeedSetting::Realtime
}
