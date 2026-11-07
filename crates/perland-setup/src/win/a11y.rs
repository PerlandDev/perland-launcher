//! The screen reader's view of the window: its title, the status text, the progress bar and a
//! button per control.

use crate::draw::{self, Ui};
use crate::model::{Action, Screen};
use crate::text;
use crate::uninstall::{UScreen, UStep};
use accesskit::{Live, Node, NodeId, Rect, Role, Toggled, TreeId, TreeInfo, TreeUpdate};

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
pub fn tree(screen: &Screen, ui: &Ui, scale: f64, title: &str) -> TreeUpdate {
    let rect = |r: tiny_skia::Rect| Rect {
        x0: r.left() as f64 * scale,
        y0: r.top() as f64 * scale,
        x1: r.right() as f64 * scale,
        y1: r.bottom() as f64 * scale,
    };
    let mut nodes = Vec::new();
    let mut children = vec![STATUS];

    let mut status = Node::new(Role::Label);
    status.set_value(draw::spoken(screen, ui));
    // Screen readers announce each change of screen; errors interrupt.
    let error = matches!(
        screen,
        Screen::Error(_) | Screen::Uninstall(UScreen::Failed { .. })
    ) || matches!(
        screen,
        Screen::Options {
            problem: Some(_),
            ..
        }
    );
    status.set_live(if error { Live::Assertive } else { Live::Polite });
    nodes.push((STATUS, status));

    let progress = match screen {
        Screen::Installing {
            progress,
            confirm_stop: false,
            step,
        } => Some((*progress, text::step_text(ui.lang, *step))),
        Screen::Uninstall(UScreen::Removing { progress, step }) => {
            let w = text::uninstall_words(ui.lang);
            Some((
                *progress,
                if *step == UStep::RemovingApp {
                    w.removing_app
                } else {
                    w.deleting_data
                },
            ))
        }
        _ => None,
    };
    if let Some((progress, label)) = progress {
        let mut bar = Node::new(Role::ProgressIndicator);
        bar.set_label(label);
        bar.set_min_numeric_value(0.0);
        bar.set_max_numeric_value(100.0);
        bar.set_numeric_value((progress as f64 * 100.0).round());
        nodes.push((PROGRESS, bar));
        children.push(PROGRESS);
    }

    for b in draw::buttons(screen, ui) {
        let id = node_of(b.action);
        let mut button = match b.style {
            draw::Style::Check(on) => {
                let mut check = Node::new(Role::CheckBox);
                check.set_toggled(if on { Toggled::True } else { Toggled::False });
                check
            }
            _ => Node::new(Role::Button),
        };
        button.set_label(b.label.clone());
        button.set_bounds(rect(b.rect));
        button.add_action(accesskit::Action::Click);
        button.add_action(accesskit::Action::Focus);
        nodes.push((id, button));
        children.push(id);
    }

    let mut root = Node::new(Role::Window);
    root.set_label(title);
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
