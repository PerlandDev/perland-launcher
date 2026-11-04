//! Lays out and paints the setup window into a pixel buffer, in the launcher's look (DESIGN.md).
//! Everything is in logical pixels; `render` scales to the window's DPI.

use crate::model::{Action, Screen, Step};
use crate::text::{self, Lang};
use fontdue::{Font, FontSettings};
use std::sync::OnceLock;
use tiny_skia::{
    Color, FillRule, FilterQuality, Paint, PathBuilder, Pixmap, PixmapPaint, Rect, Stroke,
    Transform,
};

pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 400;

// DESIGN.md colours.
const GROUND: [u8; 3] = [0x18, 0x14, 0x10];
const RAISED: [u8; 3] = [0x2e, 0x26, 0x1f];
const LINE: [u8; 3] = [0x46, 0x3a, 0x2f];
const INK: [u8; 3] = [0xf6, 0xf1, 0xe7];
const INK_DIM: [u8; 3] = [0xc5, 0xb9, 0xa8];
const AMBER: [u8; 3] = [0xff, 0xc2, 0x33];
const AMBER_HI: [u8; 3] = [0xff, 0xd7, 0x6e];

// Layout, logical pixels (spec §4, plan Task 3).
const TITLE_STRIP: f32 = 36.0;
fn close_rect() -> Rect {
    Rect::from_xywh(WIDTH as f32 - 44.0, 0.0, 44.0, TITLE_STRIP).expect("valid rect")
}
const LOGO_SIZE: f32 = 96.0;
const LOGO_TOP: f32 = 56.0;
const HEAD_Y: f32 = 176.0;
const HEAD_SIZE: f32 = 28.0;
const DETAIL_Y: f32 = 210.0;
const DETAIL_SIZE: f32 = 15.0;
const DETAIL_LINE: f32 = 22.0;
const DETAIL_MAX: f32 = 520.0;
const PRIMARY: (f32, f32, f32) = (200.0, 52.0, 256.0); // width, height, top
const PRIMARY_LABEL: f32 = 18.0;
const SECONDARY_TOP: f32 = 318.0;
const SECONDARY_SIZE: f32 = 15.0;
const BAR: (f32, f32, f32) = (400.0, 6.0, 270.0); // width, height, top
const FOOTER_Y: f32 = 374.0;
const FOOTER_SIZE: f32 = 13.0;
const TEXT_MAX: f32 = 600.0;
const RADIUS: f32 = 6.0;

pub struct Ui {
    pub lang: Lang,
    pub payload_version: String,
    pub hover: Option<Action>,
    pub focus: Option<Action>,
    /// 1.0 = fully drawn; the Opening screen fades to 0.0 before the window closes.
    pub fade: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Button {
    pub action: Action,
    pub rect: Rect,
    pub label: String,
    pub primary: bool,
}

/// One drawn line of text, with its measured width and the room it has.
#[derive(Debug, Clone, PartialEq)]
pub struct TextFit {
    pub text: String,
    pub width: f32,
    pub max: f32,
}

#[derive(Clone, Copy)]
enum Face {
    Display,
    Body,
}

fn font(face: Face) -> &'static Font {
    static DISPLAY: OnceLock<Font> = OnceLock::new();
    static BODY: OnceLock<Font> = OnceLock::new();
    let (cell, bytes): (&OnceLock<Font>, &[u8]) = match face {
        Face::Display => (&DISPLAY, include_bytes!("../assets/Bungee-Regular.ttf")),
        Face::Body => (
            &BODY,
            include_bytes!("../assets/AtkinsonHyperlegibleNext.ttf"),
        ),
    };
    cell.get_or_init(|| {
        Font::from_bytes(bytes, FontSettings::default()).expect("bundled font parses")
    })
}

fn measure(face: Face, size: f32, text: &str) -> f32 {
    let f = font(face);
    let mut width = 0.0;
    let mut prev = None;
    for c in text.chars() {
        if let Some(p) = prev {
            width += f.horizontal_kern(p, c, size).unwrap_or(0.0);
        }
        width += f.metrics(c, size).advance_width;
        prev = Some(c);
    }
    width
}

fn wrap(face: Face, size: f32, text: &str, max: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split(' ') {
        match lines.last_mut() {
            Some(line) if measure(face, size, &format!("{line} {word}")) <= max => {
                line.push(' ');
                line.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    if lines.len() == 2 {
        // Balance two lines: the split whose longer line is shortest, so no word is left alone.
        let words: Vec<&str> = text.split(' ').collect();
        let best = (1..words.len())
            .map(|i| (words[..i].join(" "), words[i..].join(" ")))
            .map(|(a, b)| {
                let widest = measure(face, size, &a).max(measure(face, size, &b));
                (widest, a, b)
            })
            .filter(|(widest, ..)| *widest <= max)
            .min_by(|x, y| x.0.total_cmp(&y.0));
        if let Some((_, a, b)) = best {
            return vec![a, b];
        }
    }
    lines
}

fn closable(screen: &Screen) -> bool {
    !matches!(
        screen,
        Screen::Opening
            | Screen::Installing {
                step: Step::Installing | Step::FinishingUp,
                ..
            }
    )
}

pub fn buttons(screen: &Screen, ui: &Ui) -> Vec<Button> {
    let mut out = Vec::new();
    let (primary, secondary) = text::button_labels(ui.lang, screen);
    if let Some(label) = primary {
        let (w, h, top) = PRIMARY;
        let rect = Rect::from_xywh((WIDTH as f32 - w) / 2.0, top, w, h).expect("valid rect");
        out.push(Button {
            action: Action::Primary,
            rect,
            label: label.into(),
            primary: true,
        });
    }
    if let Some(label) = secondary {
        let action = if matches!(screen, Screen::Error(_)) {
            Action::CopyDetails
        } else {
            Action::Secondary
        };
        let w = measure(Face::Body, SECONDARY_SIZE, label) + 24.0;
        let rect =
            Rect::from_xywh((WIDTH as f32 - w) / 2.0, SECONDARY_TOP, w, 32.0).expect("valid rect");
        out.push(Button {
            action,
            rect,
            label: label.into(),
            primary: false,
        });
    }
    if closable(screen) {
        let label = text::strings(ui.lang).close.into();
        out.push(Button {
            action: Action::Close,
            rect: close_rect(),
            label,
            primary: false,
        });
    }
    out
}

pub fn hit(screen: &Screen, ui: &Ui, x: f32, y: f32) -> Option<Action> {
    buttons(screen, ui)
        .into_iter()
        .find(|b| {
            x >= b.rect.left() && x < b.rect.right() && y >= b.rect.top() && y < b.rect.bottom()
        })
        .map(|b| b.action)
}

/// Whether a press at this height drags the window (the title strip, outside the close button).
pub fn drag_region(y: f32) -> bool {
    y < TITLE_STRIP
}

struct Line {
    text: String,
    face: Face,
    size: f32,
    center_y: f32,
    colour: [u8; 3],
    max: f32,
}

fn lines(screen: &Screen, ui: &Ui) -> Vec<Line> {
    let line = |text: String, face, size, center_y, colour, max| Line {
        text,
        face,
        size,
        center_y,
        colour,
        max,
    };
    let mut out = vec![line(
        text::headline(ui.lang, screen).into(),
        Face::Display,
        HEAD_SIZE,
        HEAD_Y,
        INK,
        TEXT_MAX,
    )];
    let detail = text::detail(ui.lang, screen);
    for (i, l) in wrap(Face::Body, DETAIL_SIZE, &detail, DETAIL_MAX)
        .into_iter()
        .enumerate()
    {
        let y = DETAIL_Y + i as f32 * DETAIL_LINE;
        out.push(line(l, Face::Body, DETAIL_SIZE, y, INK_DIM, DETAIL_MAX));
    }
    if let Some(footer) = text::footer(ui.lang, screen, &ui.payload_version) {
        out.push(line(
            footer,
            Face::Body,
            FOOTER_SIZE,
            FOOTER_Y,
            INK_DIM,
            TEXT_MAX,
        ));
    }
    out
}

pub fn texts(screen: &Screen, ui: &Ui) -> Vec<TextFit> {
    let mut out: Vec<TextFit> = lines(screen, ui)
        .into_iter()
        .map(|l| TextFit {
            width: measure(l.face, l.size, &l.text),
            text: l.text,
            max: l.max,
        })
        .collect();
    for b in buttons(screen, ui) {
        if b.action == Action::Close {
            continue;
        }
        let (face, size) = if b.primary {
            (Face::Display, PRIMARY_LABEL)
        } else {
            (Face::Body, SECONDARY_SIZE)
        };
        out.push(TextFit {
            width: measure(face, size, &b.label),
            max: b.rect.width() - 16.0,
            text: b.label,
        });
    }
    out
}

fn colour(rgb: [u8; 3], alpha: f32) -> Color {
    Color::from_rgba8(
        rgb[0],
        rgb[1],
        rgb[2],
        (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

fn paint(rgb: [u8; 3], alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(colour(rgb, alpha));
    p.anti_alias = true;
    p
}

fn rounded(rect: Rect, r: f32) -> tiny_skia::Path {
    let (l, t, rt, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    let r = r.min(rect.width() / 2.0).min(rect.height() / 2.0);
    let mut pb = PathBuilder::new();
    pb.move_to(l + r, t);
    pb.line_to(rt - r, t);
    pb.quad_to(rt, t, rt, t + r);
    pb.line_to(rt, b - r);
    pb.quad_to(rt, b, rt - r, b);
    pb.line_to(l + r, b);
    pb.quad_to(l, b, l, b - r);
    pb.line_to(l, t + r);
    pb.quad_to(l, t, l + r, t);
    pb.close();
    pb.finish().expect("valid path")
}

fn scaled(rect: Rect, scale: f32) -> Rect {
    Rect::from_ltrb(
        rect.left() * scale,
        rect.top() * scale,
        rect.right() * scale,
        rect.bottom() * scale,
    )
    .expect("valid rect")
}

/// Composites a glyph coverage mask (`size` = width, height) at device pixel `at` in `rgb`.
fn blend_mask(
    pixmap: &mut Pixmap,
    mask: &[u8],
    size: (usize, usize),
    at: (i32, i32),
    ink: ([u8; 3], f32),
) {
    let ((w, h), (x, y), (rgb, alpha)) = (size, at, ink);
    let (pw, ph) = (pixmap.width() as i32, pixmap.height() as i32);
    let data = pixmap.data_mut();
    for row in 0..h as i32 {
        for col in 0..w as i32 {
            let (px, py) = (x + col, y + row);
            if px < 0 || py < 0 || px >= pw || py >= ph {
                continue;
            }
            let a = mask[(row * w as i32 + col) as usize] as f32 / 255.0 * alpha;
            if a <= 0.0 {
                continue;
            }
            let i = ((py * pw + px) * 4) as usize;
            for (k, c) in rgb.iter().enumerate() {
                data[i + k] = (*c as f32 * a + data[i + k] as f32 * (1.0 - a)).round() as u8;
            }
        }
    }
}

/// Draws one line centred on `center_x`, its middle at the line's `center_y` (logical px).
fn draw_text(pixmap: &mut Pixmap, line: &Line, center_x: f32, alpha: f32, scale: f32) {
    let f = font(line.face);
    let px = line.size * scale;
    let width = measure(line.face, line.size, &line.text) * scale;
    let lm = f.horizontal_line_metrics(px).expect("horizontal font");
    // The middle of the ascent (with descent) sits on center_y.
    let baseline = line.center_y * scale + (lm.ascent + lm.descent) / 2.0;
    let mut pen = center_x * scale - width / 2.0;
    let mut prev = None;
    for c in line.text.chars() {
        if let Some(p) = prev {
            pen += f.horizontal_kern(p, c, px).unwrap_or(0.0);
        }
        let (m, mask) = f.rasterize(c, px);
        let gx = (pen + m.xmin as f32).round() as i32;
        let gy = (baseline - m.height as f32 - m.ymin as f32).round() as i32;
        blend_mask(
            pixmap,
            &mask,
            (m.width, m.height),
            (gx, gy),
            (line.colour, alpha),
        );
        pen += m.advance_width;
        prev = Some(c);
    }
}

/// The pinwheel `width` device pixels wide (the brand file is 512 × 433), box-filtered down.
fn logo(width: u32) -> Pixmap {
    static SOURCE: OnceLock<Pixmap> = OnceLock::new();
    let src = SOURCE.get_or_init(|| {
        Pixmap::decode_png(include_bytes!("../assets/pinwheel-on-dark-512.png"))
            .expect("bundled logo decodes")
    });
    let ratio = src.width() as f32 / width as f32;
    let height = ((src.height() as f32 / ratio).round() as u32).max(1);
    let mut out = Pixmap::new(width, height).expect("non-zero size");
    let (sw, sh, sp) = (src.width() as usize, src.height() as usize, src.data());
    let span = |i: usize, limit: usize| {
        let start = ((i as f32 * ratio) as usize).min(limit - 1);
        let end = (((i + 1) as f32 * ratio).ceil() as usize).clamp(start + 1, limit);
        start..end
    };
    let dst = out.data_mut();
    for y in 0..height as usize {
        for x in 0..width as usize {
            let (ys, xs) = (span(y, sh), span(x, sw));
            let mut acc = [0u32; 4];
            for sy in ys.clone() {
                for sx in xs.clone() {
                    let i = (sy * sw + sx) * 4;
                    for (k, a) in acc.iter_mut().enumerate() {
                        *a += sp[i + k] as u32;
                    }
                }
            }
            let n = (ys.len() * xs.len()) as u32;
            let i = (y * width as usize + x) * 4;
            for (k, a) in acc.iter().enumerate() {
                dst[i + k] = (a / n) as u8;
            }
        }
    }
    out
}

pub fn render(screen: &Screen, ui: &Ui, scale: f32) -> Pixmap {
    let (w, h) = (
        (WIDTH as f32 * scale).round() as u32,
        (HEIGHT as f32 * scale).round() as u32,
    );
    let mut pixmap = Pixmap::new(w, h).expect("non-zero size");
    pixmap.fill(colour(GROUND, 1.0));
    let a = ui.fade;
    let t = Transform::identity();

    // Logo.
    let logo = logo((LOGO_SIZE * scale).round() as u32);
    let x = ((WIDTH as f32 * scale - logo.width() as f32) / 2.0).round() as i32;
    // Centred in the 96 px logo box, so a non-square mark still sits on the layout's grid.
    let y = ((LOGO_TOP + LOGO_SIZE / 2.0) * scale - logo.height() as f32 / 2.0).round() as i32;
    let mut lp = PixmapPaint {
        opacity: a,
        quality: FilterQuality::Nearest,
        ..PixmapPaint::default()
    };
    lp.blend_mode = tiny_skia::BlendMode::SourceOver;
    pixmap.draw_pixmap(x, y, logo.as_ref(), &lp, t, None);

    for l in lines(screen, ui) {
        draw_text(&mut pixmap, &l, WIDTH as f32 / 2.0, a, scale);
    }

    if let Screen::Installing {
        progress,
        confirm_stop: false,
        ..
    } = screen
    {
        let (bw, bh, top) = BAR;
        let track = Rect::from_xywh((WIDTH as f32 - bw) / 2.0, top, bw, bh).expect("valid rect");
        pixmap.fill_path(
            &rounded(scaled(track, scale), 3.0 * scale),
            &paint(RAISED, a),
            FillRule::Winding,
            t,
            None,
        );
        let fill_w = (bw * progress.clamp(0.0, 1.0)).max(bh);
        let fill = Rect::from_xywh(track.left(), top, fill_w, bh).expect("valid rect");
        pixmap.fill_path(
            &rounded(scaled(fill, scale), 3.0 * scale),
            &paint(AMBER, a),
            FillRule::Winding,
            t,
            None,
        );
    }

    for b in buttons(screen, ui) {
        let hovered = ui.hover == Some(b.action);
        let r = scaled(b.rect, scale);
        let cx = b.rect.left() + b.rect.width() / 2.0;
        let cy = b.rect.top() + b.rect.height() / 2.0;
        match (b.action, b.primary) {
            (Action::Close, _) => {
                if hovered {
                    pixmap.fill_rect(r, &paint(RAISED, a), t, None);
                }
                let mut pb = PathBuilder::new();
                let (d, x0, y0) = (6.0 * scale, cx * scale, cy * scale);
                pb.move_to(x0 - d, y0 - d);
                pb.line_to(x0 + d, y0 + d);
                pb.move_to(x0 + d, y0 - d);
                pb.line_to(x0 - d, y0 + d);
                let stroke = Stroke {
                    width: 1.5 * scale,
                    ..Stroke::default()
                };
                let ink = if hovered { INK } else { INK_DIM };
                pixmap.stroke_path(
                    &pb.finish().expect("valid path"),
                    &paint(ink, a),
                    &stroke,
                    t,
                    None,
                );
            }
            (_, true) => {
                let fill = if hovered { AMBER_HI } else { AMBER };
                pixmap.fill_path(
                    &rounded(r, RADIUS * scale),
                    &paint(fill, a),
                    FillRule::Winding,
                    t,
                    None,
                );
                let label = Line {
                    text: b.label.clone(),
                    face: Face::Display,
                    size: PRIMARY_LABEL,
                    center_y: cy,
                    colour: GROUND,
                    max: 0.0,
                };
                draw_text(&mut pixmap, &label, cx, a, scale);
            }
            (_, false) => {
                if hovered {
                    pixmap.fill_path(
                        &rounded(r, RADIUS * scale),
                        &paint(RAISED, a),
                        FillRule::Winding,
                        t,
                        None,
                    );
                }
                let ink = if hovered { INK } else { INK_DIM };
                let label = Line {
                    text: b.label.clone(),
                    face: Face::Body,
                    size: SECONDARY_SIZE,
                    center_y: cy,
                    colour: ink,
                    max: 0.0,
                };
                draw_text(&mut pixmap, &label, cx, a, scale);
            }
        }
        if ui.focus == Some(b.action) {
            let ring = b.rect.outset(3.0, 3.0).expect("valid rect");
            let stroke = Stroke {
                width: 2.0 * scale,
                ..Stroke::default()
            };
            let path = rounded(scaled(ring, scale), (RADIUS + 3.0) * scale);
            pixmap.stroke_path(&path, &paint(INK, a), &stroke, t, None);
        }
    }
    if !closable(screen) && !matches!(screen, Screen::Opening) {
        // The close button stays in place but greyed while the installer runs (spec §4).
        let mut pb = PathBuilder::new();
        let (cx, cy, d) = (
            (close_rect().left() + 22.0) * scale,
            18.0 * scale,
            6.0 * scale,
        );
        pb.move_to(cx - d, cy - d);
        pb.line_to(cx + d, cy + d);
        pb.move_to(cx + d, cy - d);
        pb.line_to(cx - d, cy + d);
        let stroke = Stroke {
            width: 1.5 * scale,
            ..Stroke::default()
        };
        pixmap.stroke_path(
            &pb.finish().expect("valid path"),
            &paint(LINE, a),
            &stroke,
            t,
            None,
        );
    }
    pixmap
}

#[cfg(test)]
fn has_glyph(c: char) -> bool {
    c == ' '
        || (font(Face::Display).lookup_glyph_index(c) != 0
            && font(Face::Body).lookup_glyph_index(c) != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Offer, Step};
    use crate::text::all_screens;

    fn ui(lang: Lang) -> Ui {
        Ui {
            lang,
            payload_version: "0.2.0-beta.12".into(),
            hover: None,
            focus: None,
            fade: 1.0,
        }
    }

    #[test]
    fn buttons_inside_window() {
        for lang in [Lang::En, Lang::Es] {
            for screen in all_screens() {
                for b in buttons(&screen, &ui(lang)) {
                    assert!(
                        b.rect.left() >= 0.0
                            && b.rect.top() >= 0.0
                            && b.rect.right() <= WIDTH as f32
                            && b.rect.bottom() <= HEIGHT as f32,
                        "{lang:?} {screen:?} {b:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn no_text_overflows() {
        for lang in [Lang::En, Lang::Es] {
            for screen in all_screens() {
                for fit in texts(&screen, &ui(lang)) {
                    assert!(fit.width <= fit.max, "{lang:?} {screen:?} {fit:?}");
                }
            }
        }
    }

    #[test]
    fn details_wrap_to_two_lines_at_most() {
        let s = crate::text::strings(Lang::Es);
        for screen in all_screens() {
            let detail = crate::text::detail(Lang::Es, &screen);
            let lines = texts(&screen, &ui(Lang::Es))
                .into_iter()
                .filter(|f| detail.contains(f.text.as_str()) && f.text != s.brand)
                .count();
            assert!(lines <= 2, "{screen:?}");
        }
    }

    #[test]
    fn two_line_wraps_are_balanced() {
        let text = crate::text::strings(Lang::Es).running_detail;
        let lines = wrap(Face::Body, DETAIL_SIZE, text, DETAIL_MAX);
        assert_eq!(lines.len(), 2, "{lines:?}");
        let widths: Vec<f32> = lines
            .iter()
            .map(|l| measure(Face::Body, DETAIL_SIZE, l))
            .collect();
        assert!(
            widths[1] >= widths[0] * 0.6 && widths[0] >= widths[1] * 0.6,
            "{lines:?}"
        );
        assert_eq!(lines.join(" "), text);
    }

    #[test]
    fn every_character_has_a_glyph() {
        for lang in [Lang::En, Lang::Es] {
            for screen in all_screens() {
                for fit in texts(&screen, &ui(lang)) {
                    for c in fit.text.chars() {
                        assert!(has_glyph(c), "{c:?} in {:?}", fit.text);
                    }
                }
            }
        }
    }

    #[test]
    fn hit_finds_primary_and_close() {
        let screen = Screen::Ready(Offer::Install);
        let u = ui(Lang::En);
        let primary = buttons(&screen, &u)
            .into_iter()
            .find(|b| b.primary)
            .unwrap();
        let (cx, cy) = (
            primary.rect.left() + primary.rect.width() / 2.0,
            primary.rect.top() + primary.rect.height() / 2.0,
        );
        assert_eq!(hit(&screen, &u, cx, cy), Some(Action::Primary));
        assert_eq!(
            hit(&screen, &u, WIDTH as f32 - 18.0, 18.0),
            Some(Action::Close)
        );
        assert_eq!(hit(&screen, &u, 10.0, 200.0), None);
    }

    #[test]
    fn close_hidden_while_installing() {
        for step in [Step::Installing, Step::FinishingUp] {
            let screen = Screen::Installing {
                step,
                progress: 0.5,
                confirm_stop: false,
            };
            assert!(buttons(&screen, &ui(Lang::En))
                .iter()
                .all(|b| b.action != Action::Close));
        }
        assert!(buttons(&Screen::Opening, &ui(Lang::En)).is_empty());
        let ready = Screen::Installing {
            step: Step::GettingReady,
            progress: 0.0,
            confirm_stop: false,
        };
        assert!(buttons(&ready, &ui(Lang::En))
            .iter()
            .any(|b| b.action == Action::Close));
    }

    #[test]
    fn drag_region_is_the_title_strip() {
        assert!(drag_region(10.0));
        assert!(!drag_region(60.0));
    }

    #[test]
    fn render_has_expected_size() {
        let p = render(&Screen::Ready(Offer::Install), &ui(Lang::En), 1.5);
        assert_eq!((p.width(), p.height()), (960, 600));
    }

    #[test]
    fn render_paints_ground_and_amber() {
        let screen = Screen::Ready(Offer::Install);
        let u = ui(Lang::En);
        let p = render(&screen, &u, 1.0);
        let px = p.pixel(4, 390).unwrap();
        assert_eq!((px.red(), px.green(), px.blue()), (0x18, 0x14, 0x10));
        // The primary button's left edge, between the label and the rounded corner, is amber.
        let primary = buttons(&screen, &u)
            .into_iter()
            .find(|b| b.primary)
            .unwrap();
        let px = p
            .pixel(
                primary.rect.left() as u32 + 8,
                primary.rect.top() as u32 + 26,
            )
            .unwrap();
        assert_eq!((px.red(), px.green(), px.blue()), (0xff, 0xc2, 0x33));
    }
}
