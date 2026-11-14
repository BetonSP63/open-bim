use eframe::egui;

/// These UI tests have no renderer to consume texture updates. Explicitly
/// discard them while preserving accessibility and viewport output for checks.
pub trait HeadlessFrame {
    fn run_headless(
        &self,
        input: egui::RawInput,
        draw: impl FnMut(&mut egui::Ui),
    ) -> egui::FullOutput;
}

impl HeadlessFrame for egui::Context {
    fn run_headless(
        &self,
        input: egui::RawInput,
        draw: impl FnMut(&mut egui::Ui),
    ) -> egui::FullOutput {
        let mut output = self.run_ui(input, draw);
        output.textures_delta.clear();
        output
    }
}
