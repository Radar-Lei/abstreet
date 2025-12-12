use geom::{Distance, Polygon};

use crate::{
    assets::Assets, EdgeInsets, EventCtx, GeomBatch, GfxCtx, Key, Line, Outcome, ScreenDims,
    ScreenPt, ScreenRectangle, Style, Text, Widget, WidgetImpl, WidgetOutput,
};

// A multiline text input widget. Enter inserts a newline.
pub struct MultilineTextBox {
    text: String,
    label: String,
    cursor_x: usize,
    has_focus: bool,
    autofocus: bool,
    padding: EdgeInsets,

    top_left: ScreenPt,
    dims: ScreenDims,
}

impl MultilineTextBox {
    pub fn widget<I: Into<String>>(
        ctx: &EventCtx,
        label: I,
        prefilled: String,
        dims: ScreenDims,
        autofocus: bool,
    ) -> Widget {
        let label = label.into();
        Widget::new(Box::new(MultilineTextBox::new(
            ctx,
            label.clone(),
            prefilled,
            dims,
            autofocus,
        )))
        .named(label)
    }

    pub fn get_text(&self) -> String {
        self.text.clone()
    }

    pub(crate) fn new(
        _ctx: &EventCtx,
        label: String,
        prefilled: String,
        dims: ScreenDims,
        autofocus: bool,
    ) -> MultilineTextBox {
        let padding = EdgeInsets {
            top: 6.0,
            left: 8.0,
            bottom: 8.0,
            right: 8.0,
        };
        MultilineTextBox {
            label,
            cursor_x: prefilled.len(),
            text: prefilled,
            has_focus: false,
            autofocus,
            padding,
            top_left: ScreenPt::new(0.0, 0.0),
            dims,
        }
    }

    fn calculate_text(&self, style: &Style, assets: &Assets) -> Text {
        let mut s = self.text.clone();
        if self.cursor_x <= s.len() {
            s.insert(self.cursor_x, '|');
        } else {
            s.push('|');
        }
        let txt = Text::from_multiline(
            s.split('\n')
                .map(|l| Line(l).fg(style.text_primary_color))
                .collect::<Vec<_>>(),
        );
        // Wrap lines to fit inside box width.
        let limit = (self.dims.width - (self.padding.left + self.padding.right) as f64).max(1.0);
        txt.inner_wrap_to_pixels(limit, assets)
    }
}

impl WidgetImpl for MultilineTextBox {
    fn get_dims(&self) -> ScreenDims {
        self.dims
    }

    fn set_pos(&mut self, top_left: ScreenPt) {
        self.top_left = top_left;
    }

    fn event(&mut self, ctx: &mut EventCtx, output: &mut WidgetOutput) {
        if !self.autofocus && ctx.redo_mouseover() {
            if let Some(pt) = ctx.canvas.get_cursor_in_screen_space() {
                self.has_focus = ScreenRectangle::top_left(self.top_left, self.dims).contains(pt);
            } else {
                self.has_focus = false;
            }
        }

        if !self.autofocus && !self.has_focus {
            return;
        }

        if let Some(key) = ctx.input.any_pressed() {
            match key {
                Key::LeftArrow => {
                    if self.cursor_x > 0 {
                        self.cursor_x -= 1;
                    }
                }
                Key::RightArrow => {
                    self.cursor_x = (self.cursor_x + 1).min(self.text.len());
                }
                Key::Backspace => {
                    if self.cursor_x > 0 {
                        output.outcome = Outcome::Changed(self.label.clone());
                        self.text.remove(self.cursor_x - 1);
                        self.cursor_x -= 1;
                    }
                }
                Key::Enter => {
                    output.outcome = Outcome::Changed(self.label.clone());
                    self.text.insert(self.cursor_x, '\n');
                    self.cursor_x += 1;
                }
                _ => {
                    if let Some(c) = key.to_char(ctx.is_key_down(Key::LeftShift)) {
                        output.outcome = Outcome::Changed(self.label.clone());
                        self.text.insert(self.cursor_x, c);
                        self.cursor_x += 1;
                    } else {
                        ctx.input.unconsume_event();
                    }
                }
            };
        }
    }

    fn draw(&self, g: &mut GfxCtx) {
        let mut batch = GeomBatch::from(vec![(
            if self.autofocus || self.has_focus {
                g.style().field_bg
            } else {
                g.style().field_bg.dull(0.5)
            },
            Polygon::rounded_rectangle(self.dims.width, self.dims.height, 2.0),
        )]);

        let outline_style = g.style().btn_outline.outline;
        batch.push(
            outline_style.1,
            Polygon::rounded_rectangle(self.dims.width, self.dims.height, 2.0)
                .to_outline(Distance::meters(outline_style.0)),
        );

        batch.append(
            self.calculate_text(g.style(), &g.prerender.assets)
                .render_autocropped(g)
                .translate(self.padding.left, self.padding.top),
        );
        let draw = g.upload(batch);
        g.redraw_at(self.top_left, &draw);
    }
}
