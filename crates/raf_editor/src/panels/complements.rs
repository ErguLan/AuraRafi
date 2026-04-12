use egui::{Color32, RichText, Stroke, Rounding};

impl AuraRafiApp {
    /// Renderiza el manejador de extensiones y DLLs (Mods).
    pub(crate) fn show_complement_manager(&mut self, ctx: &egui::Context, is_open: &mut bool) {
        let mut open = *is_open;
        if !open {
            return;
        }

        let muted_gray = Color32::from_rgb(150, 150, 160);
        let dark_bg = Color32::from_rgb(34, 34, 38);
        let accent = crate::theme::ACCENT;

        let frame = egui::Frame::window(&ctx.style())
            .fill(Color32::from_rgb(25, 25, 25))
            .stroke(Stroke::new(1.0, Color32::from_rgb(50, 50, 50)))
            .inner_margin(12.0)
            .rounding(Rounding::same(6.0));

        egui::Window::new("Extension Manager")
            .title_bar(false)
            .resizable(true)
            .collapsible(false)
            .frame(frame)
            .default_width(450.0)
            .default_height(300.0)
            .open(&mut open)
            .show(ctx, |ui| {
                
                ui.horizontal(|ui| {
                    ui.label(RichText::new("EXTENSIONS & MODS").size(13.0).color(muted_gray).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_sized([80.0, 24.0], egui::Button::new("Close").rounding(4.0)).clicked() {
                            *is_open = false;
                        }
                    });
                });
                
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    if self.complement_registry.complements.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(40.0);
                            ui.label(RichText::new("No extensions loaded.").color(muted_gray));
                        });
                        return;
                    }

                    for comp in &self.complement_registry.complements {
                        egui::Frame::none()
                            .fill(dark_bg)
                            .rounding(4.0)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.label(RichText::new(comp.name()).strong().size(12.0).color(Color32::WHITE));
                                        
                                        let domain_str = match comp.domain() {
                                            raf_core::complement::ComplementDomain::Games => "Game Domain",
                                            raf_core::complement::ComplementDomain::Electronics => "Electronics Domain",
                                            raf_core::complement::ComplementDomain::Universal => "Universal",
                                        };
                                        ui.label(RichText::new(format!("ID: {} | Domain: {}", comp.id(), domain_str)).size(11.0).color(muted_gray));
                                    });

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(RichText::new("Active").color(accent).size(11.0));
                                    });
                                });
                            });
                        ui.add_space(4.0);
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(8.0);
                
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Open-source modules loaded dynamically.").size(11.0).color(muted_gray));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_sized([100.0, 24.0], egui::Button::new(RichText::new("Load .dll").color(accent)).rounding(4.0));
                    });
                });
            });
            
        *is_open = open;
    }
}
