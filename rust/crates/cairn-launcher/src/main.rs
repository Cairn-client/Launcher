fn main() {
    let options = eframe::NativeOptions::default();
    let _ = eframe::run_native("Cairn", options, Box::new(|_cc| Ok(Box::new(App))));
}
struct App;
impl eframe::App for App {
    fn update(&mut self, ctx: &eframe::egui::Context, _: &mut eframe::Frame) {
        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            ui.label("hi");
        });
    }
}
