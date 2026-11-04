//! The screen reader's view of the window: its title, the status text, the progress bar and a
//! button per control.

use crate::draw::{self, Ui};
use crate::model::{Action, Screen};
use crate::text;
use accesskit::{Live, Node, NodeId, Rect, Role, TreeId, TreeInfo, TreeUpdate};

const ROOT: NodeId = NodeId(1);
const STATUS: NodeId = NodeId(2);
const PROGRESS: NodeId = NodeId(3);

pub fn node_of(action: Action) -> NodeId {
    match action {
        Action::Primary => NodeId(10),
        Action::Secondary => NodeId(11),
        Action::Close => NodeId(12),
        Action::CopyDetails => NodeId(13),
        Action::Options => NodeId(14),
        Action::ChangeFolder => NodeId(15),
        Action::ToggleStartMenu => NodeId(16),
        Action::ToggleDesktop => NodeId(17),
        Action::Back => NodeId(18),
        Action::ToggleDeleteData => NodeId(19),
    }
}

pub fn action_of(node: NodeId) -> Option<Action> {
    [
        Action::Primary,
        Action::Secondary,
        Action::Close,
        Action::CopyDetails,
        Action::Options,
        Action::ChangeFolder,
        Action::ToggleStartMenu,
        Action::ToggleDesktop,
        Action::Back,
        Action::ToggleDeleteData,
    ]
    .into_iter()
    .find(|a| node_of(*a) == node)
}

/// The whole tree for a screen; `scale` maps logical layout to the window's physical pixels.
pub fn tree(screen: &Screen, ui: &Ui, scale: f64) -> TreeUpdate {
    let rect = |r: tiny_skia::Rect| Rect {
        x0: r.left() as f64 * scale,
        y0: r.top() as f64 * scale,
        x1: r.right() as f64 * scale,
        y1: r.bottom() as f64 * scale,
    };
    let mut nodes = Vec::new();
    let mut children = vec![STATUS];

    let mut status = Node::new(Role::Label);
    let mut words = format!(
        "{}. {}",
        text::headline(ui.lang, screen),
        text::detail(ui.lang, screen)
    );
    if let Some(footer) = text::footer(ui.lang, screen, &ui.payload_version) {
        words = format!("{words} {footer}");
    }
    status.set_value(words);
    // Screen readers announce each change of screen; errors interrupt.
    status.set_live(if matches!(screen, Screen::Error(_)) {
        Live::Assertive
    } else {
        Live::Polite
    });
    nodes.push((STATUS, status));

    if let Screen::Installing {
        progress,
        confirm_stop: false,
        step,
    } = screen
    {
        let mut bar = Node::new(Role::ProgressIndicator);
        bar.set_label(text::step_text(ui.lang, *step));
        bar.set_min_numeric_value(0.0);
        bar.set_max_numeric_value(100.0);
        bar.set_numeric_value((*progress as f64 * 100.0).round());
        nodes.push((PROGRESS, bar));
        children.push(PROGRESS);
    }

    for b in draw::buttons(screen, ui) {
        let id = node_of(b.action);
        let mut button = Node::new(Role::Button);
        button.set_label(b.label.clone());
        button.set_bounds(rect(b.rect));
        button.add_action(accesskit::Action::Click);
        button.add_action(accesskit::Action::Focus);
        nodes.push((id, button));
        children.push(id);
    }

    let mut root = Node::new(Role::Window);
    root.set_label(text::strings(ui.lang).window_title);
    root.set_children(children);
    nodes.push((ROOT, root));

    let focus = ui
        .focus
        .filter(|a| draw::buttons(screen, ui).iter().any(|b| b.action == *a))
        .map(node_of)
        .unwrap_or(ROOT);
    let mut info = TreeInfo::new(ROOT);
    info.toolkit_name = Some("Perland Launcher Setup".into());
    TreeUpdate {
        nodes,
        tree: Some(info),
        tree_id: TreeId::ROOT,
        focus,
    }
}
