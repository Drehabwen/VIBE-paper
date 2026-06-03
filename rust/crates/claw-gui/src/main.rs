#![windows_subsystem = "windows"]

mod app;
mod backend;
mod workspace;

fn main() -> eframe::Result {
    // Load workspace config
    let ws_config = workspace::WorkspaceConfig::load();
    let initial_workspace = ws_config.workspace_root.as_ref()
        .map(|p| std::path::PathBuf::from(p));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 500.0])
            .with_title("Galen — 医学科研助手"),
        ..Default::default()
    };

    eframe::run_native(
        "Galen",
        options,
        Box::new(move |cc| {
            let app = app::ClawMdApp::new(cc, initial_workspace.clone());
            Ok(Box::new(app))
        }),
    )
}
