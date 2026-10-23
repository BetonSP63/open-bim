//! «Визуализатор»: a ray-traced still of the current 3D view, rendered in
//! the background, shown in a window and saved as PNG next to the model.
use super::{Request, Workbench};
use eframe::egui;
use open_bim_renderer::visualize::{Image, Settings};
use std::sync::mpsc::Receiver;

/// Image sizes offered, pixels.
pub const SIZES: [[u32; 2]; 4] = [[1280, 800], [1600, 1000], [1920, 1200], [2560, 1600]];

#[derive(Default)]
pub struct Visualizer {
    pub open: bool,
    pub settings: Settings,
    /// The render running in the background.
    pub job: Option<(Receiver<Image>, std::time::Instant)>,
    pub image: Option<Image>,
    texture: Option<egui::TextureHandle>,
    pub message: Option<String>,
}

/// A free «Снимки/визуализация-N.png» next to the model file.
pub fn image_path(project_path: &str) -> std::path::PathBuf {
    let folder = match std::path::Path::new(project_path).parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(super::view_tools::SCREENSHOT_FOLDER),
        _ => std::path::PathBuf::from(super::view_tools::SCREENSHOT_FOLDER),
    };
    (1..)
        .map(|n| folder.join(format!("визуализация-{n}.png")))
        .find(|p| !p.exists())
        .expect("an unused number")
}

/// Write the image as PNG, creating the folder.
pub fn save(image: &Image, path: &std::path::Path) -> Result<(), String> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let buffer = image::RgbaImage::from_raw(image.width, image.height, image.rgba.clone())
        .ok_or("Неверный размер изображения")?;
    buffer.save(path).map_err(|e| e.to_string())
}

impl Workbench {
    pub(super) fn visualizer_window(&mut self, ctx: &egui::Context, requests: &mut Vec<Request>) {
        let v = &mut self.visualizer;
        if let Some((rx, started)) = &v.job {
            match rx.try_recv() {
                Ok(image) => {
                    v.message = Some(format!(
                        "Готово за {:.1} с",
                        started.elapsed().as_secs_f64()
                    ));
                    v.texture = Some(ctx.load_texture(
                        "visualizer",
                        egui::ColorImage::from_rgba_unmultiplied(
                            [image.width as usize, image.height as usize],
                            &image.rgba,
                        ),
                        egui::TextureOptions::LINEAR,
                    ));
                    v.image = Some(image);
                    v.job = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    v.message = Some("Визуализация прервана".into());
                    v.job = None;
                }
            }
        }
        if !v.open {
            return;
        }
        let mut open = true;
        let (mut run, mut save_now) = (false, false);
        let busy = v.job.is_some();
        egui::Window::new("Визуализатор")
            .id(egui::Id::new("visualizer"))
            .open(&mut open)
            .default_width(560.)
            .show(ctx, |ui| {
                let s = &mut v.settings;
                egui::Grid::new("visualizer grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let l = ui.label("Размер, пикс.");
                        egui::ComboBox::from_id_salt("visualizer size")
                            .selected_text(format!("{} × {}", s.width, s.height))
                            .show_ui(ui, |ui| {
                                for [w, h] in SIZES {
                                    if ui
                                        .selectable_label(
                                            s.width == w && s.height == h,
                                            format!("{w} × {h}"),
                                        )
                                        .clicked()
                                    {
                                        (s.width, s.height) = (w, h);
                                    }
                                }
                            })
                            .response
                            .labelled_by(l.id);
                        ui.end_row();
                        let l = ui.label("Сглаживание");
                        ui.add(egui::Slider::new(&mut s.samples, 1..=3).suffix("×"))
                            .labelled_by(l.id);
                        ui.end_row();
                        let l = ui.label("Солнце: азимут, °");
                        ui.add(egui::Slider::new(&mut s.sun[0], 0.0..=360.0))
                            .labelled_by(l.id);
                        ui.end_row();
                        let l = ui.label("Солнце: высота, °");
                        ui.add(egui::Slider::new(&mut s.sun[1], 5.0..=90.0))
                            .labelled_by(l.id);
                        ui.end_row();
                    });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut s.shadows, "Тени");
                    ui.checkbox(&mut s.ground, "Земля");
                });
                ui.horizontal(|ui| {
                    run = ui
                        .add_enabled(!busy, egui::Button::new("Визуализировать вид"))
                        .clicked();
                    save_now = ui
                        .add_enabled(v.image.is_some(), egui::Button::new("Сохранить PNG"))
                        .clicked();
                    if busy {
                        ui.spinner();
                        ui.label("Расчёт…");
                    }
                });
                if let Some(m) = &v.message {
                    ui.label(m);
                }
                if let Some(t) = &v.texture {
                    let width = ui.available_width();
                    let size = t.size_vec2();
                    ui.image((t.id(), size * (width / size.x)));
                }
            });
        v.open = open;
        if run {
            v.message = None;
            requests.push(Request::Visualize(v.settings));
        }
        if save_now && let Some(image) = &v.image {
            let path = image_path(&self.tools.path);
            v.message = Some(match save(image, &path) {
                Ok(()) => format!("Сохранено: {}", path.display()),
                Err(e) => format!("{}: {e}", path.display()),
            });
        }
    }
}
