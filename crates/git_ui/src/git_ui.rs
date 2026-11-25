pub mod branch_picker;
mod git_timestamp;

use gpui::App;

use workspace::Workspace;

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _, _| {
        branch_picker::register(workspace);
    })
    .detach();
}
