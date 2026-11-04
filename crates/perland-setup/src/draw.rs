//! Lays out and paints the setup window into a pixel buffer, in the launcher's look (DESIGN.md).
//! Everything is in logical pixels; `render` scales to the window's DPI.

use crate::model::{Action, Offer, Screen, Step};
use crate::options::{Choice, FolderProblem};
use crate::text::{self, Lang};
use crate::uninstall::UScreen;
use fontdue::{Font, FontSettings};
use std::path::{Path, PathBuf};
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
/// The progress bar's width (logical px); the shell redraws a tick only when the fill moves a pixel.
pub const BAR_WIDTH: f32 = 400.0;
const BAR: (f32, f32, f32) = (BAR_WIDTH, 6.0, 270.0); // width, height, top
const FOOTER_Y: f32 = 374.0;
const FOOTER_SIZE: f32 = 13.0;
const TEXT_MAX: f32 = 600.0;
const RADIUS: f32 = 6.0;

/// The folders and choices the options view and the uninstaller show (filled by the shell from
/// Windows and the state machines).
#[derive(Debug, Clone, PartialEq)]
pub struct Paths {
    /// The install folder in force: the current install, else the default.
    pub install: PathBuf,
    /// `%APPDATA%\PerlandLauncher`: games, worlds and settings.
    pub data: PathBuf,
    pub installed_bytes: Option<u64>,
    /// The setup's choices (`Machine::choice`).
    pub choice: Choice,
    /// The data folder's size, once measured (uninstaller).
    pub data_bytes: Option<u64>,
    /// The uninstaller's "also delete my data" box (`UMachine::delete_data`).
    pub delete_data: bool,
}

impl Paths {
    /// Realistic paths for renders and tests.
    pub fn preview() -> Paths {
        Paths {
            install: PathBuf::from("C:\\Users\\ana\\AppData\\Local\\Perland Launcher"),
            data: PathBuf::from("C:\\Users\\ana\\AppData\\Roaming\\PerlandLauncher"),
            installed_bytes: Some(25_400_000),
            choice: Choice::default(),
            data_bytes: Some(3_240_000_000),
            delete_data: false,
        }
    }

    /// The folder the install goes to: the chosen one, else the one in force.
    pub fn target(&self) -> &Path {
        self.choice.folder.as_deref().unwrap_or(&self.install)
    }
}

pub struct Ui {
    pub lang: Lang,
    pub payload_version: String,
    pub hover: Option<Action>,
    pub focus: Option<Action>,
    /// 1.0 = fully drawn; the Opening screen fades to 0.0 before the window closes.
    pub fade: f32,
    pub paths: Paths,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// The one amber button.
    Primary,
    /// A quiet text button (Reinstall, Back, Cancel, Copy details).
    Text,
    /// A small underlined link (Options).
    Link,
    /// A bordered button (Change…).
    Outline,
    /// A checkbox with its label; `true` when ticked.
    Check(bool),
    /// The × in the title strip.
    Close,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Button {
    pub action: Action,
    pub rect: Rect,
    pub label: String,
    pub primary: bool,
    pub style: Style,
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

/// Shortens `text` in the middle with `…` until it fits `max` at the body face's 15 px.
pub fn ellipsize_middle(text: &str, max: f32) -> String {
    ellipsize_at(Face::Body, 15.0, text, max)
}

fn ellipsize_at(face: Face, size: f32, text: &str, max: f32) -> String {
    if measure(face, size, text) <= max {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    for keep in (2..chars.len()).rev() {
        let head = keep * 2 / 5;
        let tail = keep - head;
        let candidate: String = chars[..head]
            .iter()
            .chain(std::iter::once(&'…'))
            .chain(&chars[chars.len() - tail..])
            .collect();
        if measure(face, size, &candidate) <= max {
            return candidate;
        }
    }
    "…".into()
}

fn closable(screen: &Screen) -> bool {
    !matches!(
        screen,
        Screen::Opening
            | Screen::Installing {
                step: Step::Installing | Step::FinishingUp,
                ..
            }
            | Screen::Uninstall(UScreen::Removing { .. })
    )
}

/// Whether a press at this height drags the window (the title strip, outside the close button).
pub fn drag_region(y: f32) -> bool {
    y < TITLE_STRIP
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Center,
    Left,
}

struct Line {
    text: String,
    face: Face,
    size: f32,
    /// The centre for `Align::Center`, the left edge for `Align::Left`.
    x: f32,
    align: Align,
    center_y: f32,
    colour: [u8; 3],
    max: f32,
}

impl Line {
    fn new(
        text: impl Into<String>,
        face: Face,
        size: f32,
        center_y: f32,
        colour: [u8; 3],
        max: f32,
    ) -> Line {
        Line {
            text: text.into(),
            face,
            size,
            x: WIDTH as f32 / 2.0,
            align: Align::Center,
            center_y,
            colour,
            max,
        }
    }

    fn left(mut self, x: f32) -> Line {
        self.x = x;
        self.align = Align::Left;
        self
    }

    fn width(&self) -> f32 {
        measure(self.face, self.size, &self.text)
    }

    #[cfg(test)]
    fn rect(&self) -> Rect {
        let w = self.width().max(1.0);
        let left = if self.align == Align::Left {
            self.x
        } else {
            self.x - w / 2.0
        };
        let h = self.size * 1.3;
        Rect::from_xywh(left, self.center_y - h / 2.0, w, h).expect("valid rect")
    }
}

/// Everything one screen draws, in logical pixels.
struct Layout {
    lines: Vec<Line>,
    buttons: Vec<Button>,
    progress: Option<f32>,
    logo: bool,
    /// The amber mark before a folder problem.
    marks: Vec<Rect>,
}

fn button(action: Action, rect: Rect, label: impl Into<String>, style: Style) -> Button {
    Button {
        action,
        rect,
        label: label.into(),
        primary: style == Style::Primary,
        style,
    }
}

fn xywh(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_xywh(x, y, w, h).expect("valid rect")
}

/// A text button sized to its label, its middle at `center_y`.
fn text_button(action: Action, label: &str, x: f32, center_y: f32) -> Button {
    let w = measure(Face::Body, SECONDARY_SIZE, label) + 24.0;
    button(
        action,
        xywh(x, center_y - 16.0, w, 32.0),
        label,
        Style::Text,
    )
}

/// A checkbox with its label, starting at `x`, its middle at `center_y`.
fn check_button(action: Action, label: &str, on: bool, x: f32, center_y: f32) -> Button {
    // 2 px of slack, so float rounding of the label's width never reads as an overflow.
    let w = CHECK_GAP + measure(Face::Body, SECONDARY_SIZE, label) + 2.0;
    button(
        action,
        xywh(x, center_y - 12.0, w, 24.0),
        label,
        Style::Check(on),
    )
}

const CHECK: f32 = 18.0;
const CHECK_GAP: f32 = 28.0;
const SIDE: f32 = 40.0;
const PATH_SIZE: f32 = 15.0;
const SMALL: f32 = 13.0;

fn layout(screen: &Screen, ui: &Ui) -> Layout {
    let mut l = match screen {
        Screen::Options { offer, problem } => options_layout(offer, *problem, ui),
        Screen::Uninstall(UScreen::Confirm) => confirm_layout(ui),
        _ => centered_layout(screen, ui),
    };
    if closable(screen) {
        l.buttons.push(button(
            Action::Close,
            close_rect(),
            text::strings(ui.lang).close,
            Style::Close,
        ));
    }
    l
}

fn centered_layout(screen: &Screen, ui: &Ui) -> Layout {
    let mut lines = vec![Line::new(
        text::headline(ui.lang, screen),
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
        lines.push(Line::new(
            l,
            Face::Body,
            DETAIL_SIZE,
            y,
            INK_DIM,
            DETAIL_MAX,
        ));
    }
    let mut buttons = Vec::new();
    let (primary, secondary) = text::button_labels(ui.lang, screen);
    if let Some(label) = primary {
        let (w, h, top) = PRIMARY;
        buttons.push(button(
            Action::Primary,
            xywh((WIDTH as f32 - w) / 2.0, top, w, h),
            label,
            Style::Primary,
        ));
    }
    if let Some(label) = secondary {
        let action = match screen {
            Screen::Error(_) | Screen::Uninstall(UScreen::Failed { .. }) => Action::CopyDetails,
            _ => Action::Secondary,
        };
        let w = measure(Face::Body, SECONDARY_SIZE, label) + 24.0;
        buttons.push(text_button(
            action,
            label,
            (WIDTH as f32 - w) / 2.0,
            SECONDARY_TOP + 16.0,
        ));
    }
    // On the uninstaller's last screens the kept folder, or what couldn't be deleted, goes right
    // under the sentence that names it; elsewhere the bottom line is the setup's footer.
    let under = match screen {
        Screen::Uninstall(UScreen::Done { kept_data: true }) => {
            Some(ui.paths.data.display().to_string())
        }
        Screen::Uninstall(UScreen::Failed { not_deleted, .. }) if !not_deleted.is_empty() => {
            Some(not_deleted.join(", "))
        }
        _ => None,
    };
    if let Some(under) = under {
        let y = DETAIL_Y + (lines.len() - 1) as f32 * DETAIL_LINE;
        let text = ellipsize_at(Face::Body, DETAIL_SIZE, &under, TEXT_MAX);
        lines.push(Line::new(text, Face::Body, DETAIL_SIZE, y, INK, TEXT_MAX));
    }
    let footer = text::footer(ui.lang, screen, &ui.payload_version);
    let with_link = matches!(screen, Screen::Ready(offer) if *offer != Offer::Newer);
    if let Some(footer) = footer {
        if with_link {
            let link = text::options_words(ui.lang).link;
            let (fw, lw) = (
                measure(Face::Body, FOOTER_SIZE, &footer),
                measure(Face::Body, FOOTER_SIZE, link),
            );
            let start = (WIDTH as f32 - (fw + 14.0 + lw)) / 2.0;
            lines.push(
                Line::new(footer, Face::Body, FOOTER_SIZE, FOOTER_Y, INK_DIM, TEXT_MAX).left(start),
            );
            let rect = xywh(start + fw + 8.0, FOOTER_Y - 12.0, lw + 12.0, 24.0);
            buttons.push(button(Action::Options, rect, link, Style::Link));
        } else {
            let footer = ellipsize_at(Face::Body, FOOTER_SIZE, &footer, TEXT_MAX);
            lines.push(Line::new(
                footer,
                Face::Body,
                FOOTER_SIZE,
                FOOTER_Y,
                INK_DIM,
                TEXT_MAX,
            ));
        }
    }
    let progress = match screen {
        Screen::Installing {
            progress,
            confirm_stop: false,
            ..
        } => Some(*progress),
        Screen::Uninstall(UScreen::Removing { progress, .. }) => Some(*progress),
        _ => None,
    };
    Layout {
        lines,
        buttons,
        progress,
        logo: true,
        marks: vec![],
    }
}

fn options_layout(offer: &Offer, problem: Option<FolderProblem>, ui: &Ui) -> Layout {
    let (lang, w) = (ui.lang, text::options_words(ui.lang));
    let fresh = *offer == Offer::Install;
    let full = WIDTH as f32 - 2.0 * SIDE;
    let mut lines = vec![Line::new(w.title, Face::Display, 20.0, 46.0, INK, full).left(SIDE)];
    let mut buttons = Vec::new();
    let mut marks = Vec::new();

    lines.push(Line::new(w.install_to, Face::Body, SMALL, 76.0, INK_DIM, full).left(SIDE));
    let path_room = if fresh { full - 112.0 } else { full };
    let path = ellipsize_at(
        Face::Body,
        PATH_SIZE,
        &ui.paths.target().display().to_string(),
        path_room,
    );
    lines.push(Line::new(path, Face::Body, PATH_SIZE, 97.0, INK, path_room).left(SIDE));
    if fresh {
        let cw = (measure(Face::Body, SECONDARY_SIZE, w.change) + 28.0).max(96.0);
        let rect = xywh(WIDTH as f32 - SIDE - cw, 82.0, cw, 30.0);
        buttons.push(button(Action::ChangeFolder, rect, w.change, Style::Outline));
    }
    let info: (String, [u8; 3]) = match (problem, fresh) {
        (Some(p), _) => (text::folder_problem_text(lang, p).into(), INK),
        (None, true) => (text::size_line(lang, ui.paths.installed_bytes), INK_DIM),
        (None, false) => (w.update_folder.into(), INK_DIM),
    };
    let info_x = if problem.is_some() { SIDE + 12.0 } else { SIDE };
    let info_lines = wrap(Face::Body, SMALL, &info.0, full - 12.0);
    for (i, text) in info_lines.iter().take(2).enumerate() {
        let y = 121.0 + i as f32 * 17.0;
        lines.push(Line::new(text.clone(), Face::Body, SMALL, y, info.1, full - 12.0).left(info_x));
    }
    if problem.is_some() {
        let h = 17.0 * info_lines.len().min(2) as f32 - 1.0;
        marks.push(xywh(SIDE, 121.0 - 8.0, 3.0, h));
    }

    lines.push(Line::new(w.data_stays, Face::Body, SMALL, 162.0, INK_DIM, full).left(SIDE));
    let data = ellipsize_at(
        Face::Body,
        PATH_SIZE,
        &ui.paths.data.display().to_string(),
        full,
    );
    lines.push(Line::new(data, Face::Body, PATH_SIZE, 182.0, INK, full).left(SIDE));

    lines.push(Line::new(w.what, Face::Body, SMALL, 210.0, INK_DIM, full).left(SIDE));
    for (i, item) in w.items.iter().enumerate() {
        let y = 230.0 + i as f32 * 19.0;
        lines.push(Line::new(*item, Face::Body, 14.0, y, INK, full - 14.0).left(SIDE + 14.0));
        marks.push(xywh(SIDE + 2.0, y - 2.0, 4.0, 4.0));
    }

    let checks_y = 314.0;
    if fresh {
        let first = check_button(
            Action::ToggleStartMenu,
            w.start_menu,
            ui.paths.choice.start_menu,
            SIDE,
            checks_y,
        );
        let second_x = first.rect.right() + 32.0;
        buttons.push(first);
        buttons.push(check_button(
            Action::ToggleDesktop,
            w.desktop,
            ui.paths.choice.desktop,
            second_x,
            checks_y,
        ));
    } else {
        lines.push(
            Line::new(w.shortcuts_stay, Face::Body, SMALL, checks_y, INK_DIM, full).left(SIDE),
        );
    }

    let (primary, secondary) = text::button_labels(
        lang,
        &Screen::Options {
            offer: offer.clone(),
            problem,
        },
    );
    let px = 170.0;
    if let Some(label) = primary {
        buttons.push(button(
            Action::Primary,
            xywh(px, 338.0, 200.0, 48.0),
            label,
            Style::Primary,
        ));
    }
    if let Some(label) = secondary {
        buttons.push(text_button(Action::Back, label, px + 216.0, 362.0));
    }
    // Bullets are drawn as dim squares; the problem mark is amber (see render).
    Layout {
        lines,
        buttons,
        progress: None,
        logo: false,
        marks,
    }
}

fn confirm_layout(ui: &Ui) -> Layout {
    let (lang, w) = (ui.lang, text::uninstall_words(ui.lang));
    let screen = Screen::Uninstall(UScreen::Confirm);
    let mut lines = vec![Line::new(
        w.title,
        Face::Display,
        HEAD_SIZE,
        HEAD_Y,
        INK,
        TEXT_MAX,
    )];
    lines.push(Line::new(
        w.removes_from,
        Face::Body,
        SMALL,
        204.0,
        INK_DIM,
        TEXT_MAX,
    ));
    let path = ellipsize_at(
        Face::Body,
        PATH_SIZE,
        &ui.paths.install.display().to_string(),
        TEXT_MAX,
    );
    lines.push(Line::new(path, Face::Body, PATH_SIZE, 224.0, INK, TEXT_MAX));
    lines.push(Line::new(
        w.removes_also,
        Face::Body,
        SMALL,
        245.0,
        INK_DIM,
        TEXT_MAX,
    ));

    let mut buttons = Vec::new();
    let cw = CHECK_GAP + measure(Face::Body, SECONDARY_SIZE, w.delete_box);
    buttons.push(check_button(
        Action::ToggleDeleteData,
        w.delete_box,
        ui.paths.delete_data,
        (WIDTH as f32 - cw) / 2.0,
        273.0,
    ));
    let size = ui.paths.data_bytes.map(|b| text::bytes_text(lang, b));
    let size_w = size
        .as_deref()
        .map_or(0.0, |s| measure(Face::Body, SMALL, &format!(" · {s}")));
    let data = ellipsize_at(
        Face::Body,
        SMALL,
        &ui.paths.data.display().to_string(),
        TEXT_MAX - size_w,
    );
    let data = match size {
        Some(s) => format!("{data} · {s}"),
        None => data,
    };
    lines.push(Line::new(data, Face::Body, SMALL, 296.0, INK_DIM, TEXT_MAX));

    let (_, secondary) = text::button_labels(lang, &screen);
    let primary = if ui.paths.delete_data {
        w.uninstall_delete
    } else {
        w.uninstall
    };
    let px = 130.0;
    buttons.push(button(
        Action::Primary,
        xywh(px, 318.0, 280.0, 48.0),
        primary,
        Style::Primary,
    ));
    if let Some(label) = secondary {
        buttons.push(text_button(Action::Secondary, label, px + 296.0, 342.0));
    }
    Layout {
        lines,
        buttons,
        progress: None,
        logo: true,
        marks: vec![],
    }
}

pub fn buttons(screen: &Screen, ui: &Ui) -> Vec<Button> {
    layout(screen, ui).buttons
}

pub fn hit(screen: &Screen, ui: &Ui, x: f32, y: f32) -> Option<Action> {
    buttons(screen, ui)
        .into_iter()
        .find(|b| {
            x >= b.rect.left() && x < b.rect.right() && y >= b.rect.top() && y < b.rect.bottom()
        })
        .map(|b| b.action)
}

pub fn texts(screen: &Screen, ui: &Ui) -> Vec<TextFit> {
    let l = layout(screen, ui);
    let mut out: Vec<TextFit> = l
        .lines
        .iter()
        .map(|line| TextFit {
            width: line.width(),
            text: line.text.clone(),
            max: line.max,
        })
        .collect();
    for b in l.buttons {
        let (face, size, room) = match b.style {
            Style::Close => continue,
            Style::Primary => (Face::Display, PRIMARY_LABEL, b.rect.width() - 16.0),
            Style::Check(_) => (Face::Body, SECONDARY_SIZE, b.rect.width() - CHECK_GAP),
            Style::Link => (Face::Body, FOOTER_SIZE, b.rect.width() - 8.0),
            Style::Text | Style::Outline => (Face::Body, SECONDARY_SIZE, b.rect.width() - 16.0),
        };
        out.push(TextFit {
            width: measure(face, size, &b.label),
            max: room,
            text: b.label,
        });
    }
    out
}

/// Every drawn line's and control's box, for the overlap test.
#[cfg(test)]
fn boxes(screen: &Screen, ui: &Ui) -> Vec<Rect> {
    let l = layout(screen, ui);
    l.lines
        .iter()
        .map(Line::rect)
        .chain(l.buttons.iter().map(|b| b.rect))
        .collect()
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

/// Draws one line at its own `x` (centre or left edge), its middle at `center_y` (logical px).
fn draw_text(pixmap: &mut Pixmap, line: &Line, alpha: f32, scale: f32) {
    let f = font(line.face);
    let px = line.size * scale;
    let width = measure(line.face, line.size, &line.text) * scale;
    let lm = f.horizontal_line_metrics(px).expect("horizontal font");
    // The middle of the ascent (with descent) sits on center_y.
    let baseline = line.center_y * scale + (lm.ascent + lm.descent) / 2.0;
    let mut pen = match line.align {
        Align::Center => line.x * scale - width / 2.0,
        Align::Left => line.x * scale,
    };
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

/// The pinwheel `width` device pixels wide, kept for the last width asked (it is redrawn every
/// animation frame at the same scale).
fn logo(width: u32) -> Pixmap {
    static CACHE: std::sync::Mutex<Option<Pixmap>> = std::sync::Mutex::new(None);
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    match cache.as_ref() {
        Some(p) if p.width() == width => p.clone(),
        _ => {
            let p = scaled_logo(width);
            *cache = Some(p.clone());
            p
        }
    }
}

/// The pinwheel `width` device pixels wide (the brand file is 512 × 433), box-filtered down.
fn scaled_logo(width: u32) -> Pixmap {
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

fn cross(pixmap: &mut Pixmap, cx: f32, cy: f32, scale: f32, ink: [u8; 3], a: f32) {
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
    pixmap.stroke_path(
        &pb.finish().expect("valid path"),
        &paint(ink, a),
        &stroke,
        Transform::identity(),
        None,
    );
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
    let l = layout(screen, ui);

    if l.logo {
        let logo = logo((LOGO_SIZE * scale).round() as u32);
        let x = ((WIDTH as f32 * scale - logo.width() as f32) / 2.0).round() as i32;
        // Centred in the 96 px logo box, so a non-square mark still sits on the layout's grid.
        let y = ((LOGO_TOP + LOGO_SIZE / 2.0) * scale - logo.height() as f32 / 2.0).round() as i32;
        let lp = PixmapPaint {
            opacity: a,
            quality: FilterQuality::Nearest,
            ..PixmapPaint::default()
        };
        pixmap.draw_pixmap(x, y, logo.as_ref(), &lp, t, None);
    }

    for line in &l.lines {
        draw_text(&mut pixmap, line, a, scale);
    }
    for mark in &l.marks {
        // A tall mark is a folder problem's amber bar; a small square is a list bullet.
        let ink = if mark.height() > 6.0 { AMBER } else { INK_DIM };
        pixmap.fill_rect(scaled(*mark, scale), &paint(ink, a), t, None);
    }

    if let Some(progress) = l.progress {
        let (bw, bh, top) = BAR;
        let track = xywh((WIDTH as f32 - bw) / 2.0, top, bw, bh);
        pixmap.fill_path(
            &rounded(scaled(track, scale), 3.0 * scale),
            &paint(RAISED, a),
            FillRule::Winding,
            t,
            None,
        );
        let fill = xywh(
            track.left(),
            top,
            (bw * progress.clamp(0.0, 1.0)).max(bh),
            bh,
        );
        pixmap.fill_path(
            &rounded(scaled(fill, scale), 3.0 * scale),
            &paint(AMBER, a),
            FillRule::Winding,
            t,
            None,
        );
    }

    for b in &l.buttons {
        let hovered = ui.hover == Some(b.action);
        let r = scaled(b.rect, scale);
        let cx = b.rect.left() + b.rect.width() / 2.0;
        let cy = b.rect.top() + b.rect.height() / 2.0;
        let label = |face, size, colour| Line::new(b.label.clone(), face, size, cy, colour, 0.0);
        match b.style {
            Style::Close => {
                if hovered {
                    pixmap.fill_rect(r, &paint(RAISED, a), t, None);
                }
                cross(
                    &mut pixmap,
                    cx,
                    cy,
                    scale,
                    if hovered { INK } else { INK_DIM },
                    a,
                );
            }
            Style::Primary => {
                let fill = if hovered { AMBER_HI } else { AMBER };
                pixmap.fill_path(
                    &rounded(r, RADIUS * scale),
                    &paint(fill, a),
                    FillRule::Winding,
                    t,
                    None,
                );
                let mut text = label(Face::Display, PRIMARY_LABEL, GROUND);
                text.x = cx;
                draw_text(&mut pixmap, &text, a, scale);
            }
            Style::Text | Style::Outline => {
                if hovered {
                    pixmap.fill_path(
                        &rounded(r, RADIUS * scale),
                        &paint(RAISED, a),
                        FillRule::Winding,
                        t,
                        None,
                    );
                }
                if b.style == Style::Outline {
                    let stroke = Stroke {
                        width: 1.0 * scale,
                        ..Stroke::default()
                    };
                    let inner = b.rect.outset(-0.5, -0.5).expect("valid rect");
                    pixmap.stroke_path(
                        &rounded(scaled(inner, scale), RADIUS * scale),
                        &paint(LINE, a),
                        &stroke,
                        t,
                        None,
                    );
                }
                let mut text = label(
                    Face::Body,
                    SECONDARY_SIZE,
                    if hovered { INK } else { INK_DIM },
                );
                text.x = cx;
                draw_text(&mut pixmap, &text, a, scale);
            }
            Style::Link => {
                let ink = if hovered { INK } else { INK_DIM };
                let mut text = label(Face::Body, FOOTER_SIZE, ink);
                text.x = cx;
                draw_text(&mut pixmap, &text, a, scale);
                let tw = measure(Face::Body, FOOTER_SIZE, &b.label);
                let underline = xywh(cx - tw / 2.0, cy + 7.0, tw, 1.0);
                pixmap.fill_rect(scaled(underline, scale), &paint(ink, a), t, None);
            }
            Style::Check(on) => {
                let boxed = xywh(b.rect.left(), cy - CHECK / 2.0, CHECK, CHECK);
                if on {
                    pixmap.fill_path(
                        &rounded(scaled(boxed, scale), 3.0 * scale),
                        &paint(AMBER, a),
                        FillRule::Winding,
                        t,
                        None,
                    );
                    let mut pb = PathBuilder::new();
                    let (x0, y0) = (boxed.left() * scale, boxed.top() * scale);
                    pb.move_to(x0 + 4.0 * scale, y0 + 9.5 * scale);
                    pb.line_to(x0 + 7.5 * scale, y0 + 13.0 * scale);
                    pb.line_to(x0 + 14.0 * scale, y0 + 5.5 * scale);
                    let stroke = Stroke {
                        width: 2.0 * scale,
                        ..Stroke::default()
                    };
                    pixmap.stroke_path(
                        &pb.finish().expect("valid path"),
                        &paint(GROUND, a),
                        &stroke,
                        t,
                        None,
                    );
                } else {
                    let stroke = Stroke {
                        width: 1.5 * scale,
                        ..Stroke::default()
                    };
                    let inner = boxed.outset(-0.75, -0.75).expect("valid rect");
                    let ink = if hovered { INK_DIM } else { LINE };
                    pixmap.stroke_path(
                        &rounded(scaled(inner, scale), 3.0 * scale),
                        &paint(ink, a),
                        &stroke,
                        t,
                        None,
                    );
                }
                let text = label(Face::Body, SECONDARY_SIZE, INK).left(b.rect.left() + CHECK_GAP);
                draw_text(&mut pixmap, &text, a, scale);
            }
        }
        if ui.focus == Some(b.action) {
            let ring = b.rect.outset(3.0, 3.0).expect("valid rect");
            let stroke = Stroke {
                width: 2.0 * scale,
                ..Stroke::default()
            };
            pixmap.stroke_path(
                &rounded(scaled(ring, scale), (RADIUS + 3.0) * scale),
                &paint(INK, a),
                &stroke,
                t,
                None,
            );
        }
    }
    if !closable(screen) && !matches!(screen, Screen::Opening) {
        // The close button stays in place but greyed while an install or removal runs (spec §4).
        cross(
            &mut pixmap,
            close_rect().left() + 22.0,
            18.0,
            scale,
            LINE,
            a,
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
    use crate::uninstall::UScreen;
    use std::path::PathBuf;

    fn ui(lang: Lang) -> Ui {
        Ui {
            lang,
            payload_version: "0.2.0-beta.12".into(),
            hover: None,
            focus: None,
            fade: 1.0,
            paths: Paths::preview(),
        }
    }

    fn long_ui(lang: Lang) -> Ui {
        let mut u = ui(lang);
        let deep = format!(
            "C:\\Users\\{}\\Perland Launcher",
            "verylongname ".repeat(20)
        );
        u.paths.install = PathBuf::from(&deep);
        u.paths.data = PathBuf::from(deep.replace("Perland Launcher", "PerlandLauncher"));
        u.paths.choice.folder = Some(PathBuf::from(deep));
        u
    }

    fn rect_of(screen: &Screen, u: &Ui, action: Action) -> Option<Rect> {
        buttons(screen, u)
            .into_iter()
            .find(|b| b.action == action)
            .map(|b| b.rect)
    }

    fn centre(r: Rect) -> (f32, f32) {
        (r.left() + r.width() / 2.0, r.top() + r.height() / 2.0)
    }

    fn options(offer: Offer) -> Screen {
        Screen::Options {
            offer,
            problem: None,
        }
    }

    #[test]
    fn ellipsize_keeps_both_ends() {
        let long = format!("C:\\Users\\{}\\Perland Launcher", "a".repeat(180));
        let e = ellipsize_middle(&long, 300.0);
        assert!(measure(Face::Body, 15.0, &e) <= 300.0, "{e}");
        assert!(e.starts_with("C:\\Users"), "{e}");
        assert!(e.ends_with("Perland Launcher"), "{e}");
        assert!(e.contains('…'));
        assert_eq!(ellipsize_middle("C:\\Short", 300.0), "C:\\Short");
    }

    #[test]
    fn long_paths_fit_options_view() {
        for lang in [Lang::En, Lang::Es] {
            for screen in all_screens() {
                for fit in texts(&screen, &long_ui(lang)) {
                    assert!(fit.width <= fit.max, "{lang:?} {screen:?} {fit:?}");
                }
            }
        }
    }

    #[test]
    fn options_link_on_ready() {
        let u = ui(Lang::En);
        for offer in [
            Offer::Install,
            Offer::Update {
                from: "0.1.0".into(),
            },
            Offer::Reinstall,
        ] {
            let screen = Screen::Ready(offer);
            let r = rect_of(&screen, &u, Action::Options).expect("an Options link");
            let (x, y) = centre(r);
            assert_eq!(hit(&screen, &u, x, y), Some(Action::Options));
        }
        assert!(rect_of(&Screen::Ready(Offer::Newer), &u, Action::Options).is_none());
    }

    #[test]
    fn checkboxes_are_hit() {
        let u = ui(Lang::En);
        let screen = options(Offer::Install);
        for action in [
            Action::ToggleStartMenu,
            Action::ToggleDesktop,
            Action::ChangeFolder,
        ] {
            let (x, y) = centre(rect_of(&screen, &u, action).expect("present"));
            assert_eq!(hit(&screen, &u, x, y), Some(action));
        }
        let confirm = Screen::Uninstall(UScreen::Confirm);
        let (x, y) = centre(rect_of(&confirm, &u, Action::ToggleDeleteData).expect("present"));
        assert_eq!(hit(&confirm, &u, x, y), Some(Action::ToggleDeleteData));
        let check = buttons(&screen, &u)
            .into_iter()
            .find(|b| b.action == Action::ToggleDesktop)
            .unwrap();
        assert_eq!(check.style, Style::Check(true));
    }

    #[test]
    fn update_options_have_no_checkboxes_or_change() {
        let u = ui(Lang::En);
        for offer in [
            Offer::Update {
                from: "0.1.0".into(),
            },
            Offer::Reinstall,
        ] {
            let screen = options(offer);
            for action in [
                Action::ToggleStartMenu,
                Action::ToggleDesktop,
                Action::ChangeFolder,
            ] {
                assert!(
                    rect_of(&screen, &u, action).is_none(),
                    "{screen:?} {action:?}"
                );
            }
        }
    }

    #[test]
    fn delete_button_label_follows_the_box() {
        let mut u = ui(Lang::En);
        let confirm = Screen::Uninstall(UScreen::Confirm);
        let label = |u: &Ui| {
            buttons(&confirm, u)
                .into_iter()
                .find(|b| b.primary)
                .unwrap()
                .label
        };
        assert_eq!(label(&u), "UNINSTALL");
        u.paths.delete_data = true;
        assert_eq!(label(&u), "UNINSTALL AND DELETE");
        let check = buttons(&confirm, &u)
            .into_iter()
            .find(|b| b.action == Action::ToggleDeleteData)
            .unwrap();
        assert_eq!(check.style, Style::Check(true));
    }

    #[test]
    fn no_overlap_in_options_and_confirm() {
        let problem = Screen::Options {
            offer: Offer::Install,
            problem: Some(crate::options::FolderProblem::NeedsAdmin),
        };
        for lang in [Lang::En, Lang::Es] {
            for u in [ui(lang), long_ui(lang)] {
                for screen in [
                    options(Offer::Install),
                    problem.clone(),
                    options(Offer::Reinstall),
                    Screen::Uninstall(UScreen::Confirm),
                ] {
                    let boxes = boxes(&screen, &u);
                    for (i, a) in boxes.iter().enumerate() {
                        for b in &boxes[i + 1..] {
                            let overlap = a.right().min(b.right()) - a.left().max(b.left()) > 0.5
                                && a.bottom().min(b.bottom()) - a.top().max(b.top()) > 0.5;
                            assert!(!overlap, "{lang:?} {screen:?}: {a:?} and {b:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn no_overlap_on_any_screen() {
        for lang in [Lang::En, Lang::Es] {
            for u in [ui(lang), long_ui(lang)] {
                for screen in all_screens() {
                    let boxes = boxes(&screen, &u);
                    for (i, a) in boxes.iter().enumerate() {
                        for b in &boxes[i + 1..] {
                            let overlap = a.right().min(b.right()) - a.left().max(b.left()) > 0.5
                                && a.bottom().min(b.bottom()) - a.top().max(b.top()) > 0.5;
                            assert!(!overlap, "{lang:?} {screen:?}: {a:?} and {b:?}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn options_view_has_no_logo() {
        let u = ui(Lang::En);
        let p = render(&options(Offer::Install), &u, 1.0);
        // The logo's centre on the Ready screen is plot colour; on Options it is ground or text.
        let px = p.pixel(320, 104).unwrap();
        assert_ne!((px.red(), px.green(), px.blue()), (0x3b, 0x9a, 0xe0));
        assert!(layout(&options(Offer::Install), &u).progress.is_none());
    }

    #[test]
    fn removing_shows_a_bar_and_greys_close() {
        let u = ui(Lang::En);
        let removing = Screen::Uninstall(UScreen::Removing {
            step: crate::uninstall::UStep::RemovingApp,
            progress: 0.35,
        });
        assert_eq!(layout(&removing, &u).progress, Some(0.35));
        assert!(rect_of(&removing, &u, Action::Close).is_none());
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
