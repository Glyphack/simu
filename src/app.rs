use egui::{
    Align, Button, Color32, ImageSource, Layout, Pos2, Sense, Ui, Vec2, Widget, include_image, vec2,
};

#[derive(Debug, Clone, Copy)]
pub enum PinKind {
    Input,
    Output,
}

#[derive(Debug, Clone, Copy)]
pub struct PinInfo {
    pub kind: PinKind,
    pub offset: Vec2,
}

pub struct GateGraphics {
    // TODO: Figure out what is the correct way to deal with images
    pub svg: ImageSource<'static>,
    pub pins: &'static [PinInfo],
}

pub static NAND_GRAPHICS: GateGraphics = GateGraphics {
    svg: include_image!("../assets/nand.svg"),
    pins: &[
        PinInfo {
            kind: PinKind::Input,
            offset: Vec2::new(-20.0, -7.0),
        },
        PinInfo {
            kind: PinKind::Input,
            offset: Vec2::new(-20.0, 7.0),
        },
        PinInfo {
            kind: PinKind::Output,
            offset: Vec2::new(24.0, 0.0),
        },
    ],
};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub enum LogicGate {
    Nand,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct GateInstance {
    pub id: u32,
    pub gate_type: LogicGate,
    pub position: Pos2,
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
#[serde(default)]
pub struct TemplateApp {
    /// State
    /// Gates placed on the canvas
    canvas_gates: Vec<GateInstance>,
    /// Next unique ID for gates
    next_gate_id: u32,

    /// Dragging from panel
    panel_dragged_gate: Option<LogicGate>,
    /// Temporary drag position if dragging palette-gate
    panel_drag_position: Option<Pos2>,

    /// Currently dragged gate id from canvas
    dragged_canvas_gate: Option<u32>,
    /// Offset from mouse pointer to gate center at drag start
    drag_offset: Option<Vec2>,
}

impl TemplateApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
    }

    pub fn main_layout(&mut self, ui: &mut Ui) {
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            ui.vertical(|ui| {
                ui.heading("Logic Gates");
                self.draw_palette(ui);
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.heading("Canvas");
                self.draw_canvas(ui);
            });
        });
    }

    fn draw_palette(&mut self, ui: &mut Ui) {
        let image = egui::Image::new(NAND_GRAPHICS.svg.clone()).max_height(70.0);
        let response = ui.add(egui::ImageButton::new(image).sense(Sense::click_and_drag()));

        if response.drag_started() {
            self.panel_dragged_gate = Some(LogicGate::Nand);
            self.panel_drag_position = None;
        }
        if response.dragged() {
            if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
                self.panel_drag_position = Some(pointer_pos);
            }
        }
        if Button::new("Clear Canvas")
            .min_size(vec2(48.0, 30.0))
            .ui(ui)
            .clicked()
        {
            self.canvas_gates.clear();
            self.dragged_canvas_gate = None;
            self.drag_offset = None;
        }
    }

    fn draw_canvas(&mut self, ui: &mut Ui) {
        let (resp, _painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
        let rect = resp.rect;

        // handle dragging from panel
        if let (Some(gate_type), Some(pos)) = (&self.panel_dragged_gate, self.panel_drag_position) {
            if rect.contains(pos) {
                Self::draw_gate_svg_with_pins(ui, gate_type, pos, Color32::YELLOW);
            }
        }

        // spawn a new gate
        if let (Some(gate_type), Some(pos)) = (&self.panel_dragged_gate, self.panel_drag_position) {
            if rect.contains(pos) && ui.ctx().input(|i| i.pointer.any_released()) {
                self.canvas_gates.push(GateInstance {
                    id: self.next_gate_id,
                    gate_type: gate_type.clone(),
                    position: pos,
                });
                self.next_gate_id += 1;
                self.panel_dragged_gate = None;
                self.panel_drag_position = None;
            }
        }

        let pointer_pos = ui.input(|i| i.pointer.interact_pos());
        let pointer_pressed = ui.input(|i| i.pointer.primary_down());
        let pointer_any_up = ui.input(|i| i.pointer.any_released());
        if self.panel_dragged_gate.is_none()
            && self.dragged_canvas_gate.is_none()
            && pointer_pressed
        {
            if let Some(mouse_pos) = pointer_pos {
                for gate in self.canvas_gates.iter().rev() {
                    let size = Vec2::new(48.0, 36.0);
                    let gate_rect = egui::Rect::from_center_size(gate.position, size);
                    if gate_rect.contains(mouse_pos) {
                        self.dragged_canvas_gate = Some(gate.id);
                        self.drag_offset = Some(gate.position.to_vec2() - mouse_pos.to_vec2());
                        break;
                    }
                }
            }
        }
        if let (Some(drag_id), Some(mouse_pos), Some(offset)) =
            (self.dragged_canvas_gate, pointer_pos, self.drag_offset)
        {
            for gate in &mut self.canvas_gates {
                if gate.id == drag_id {
                    let mut new_pos = mouse_pos + offset;
                    new_pos.x = new_pos.x.clamp(rect.left() + 24.0, rect.right() - 24.0);
                    new_pos.y = new_pos.y.clamp(rect.top() + 18.0, rect.bottom() - 18.0);
                    gate.position = new_pos;
                }
            }
            if pointer_any_up {
                self.dragged_canvas_gate = None;
                self.drag_offset = None;
            }
        }

        for gate in &self.canvas_gates {
            let highlight = self.dragged_canvas_gate == Some(gate.id);
            let tint = if highlight {
                Color32::YELLOW
            } else {
                Color32::LIGHT_BLUE
            };
            Self::draw_gate_svg_with_pins(ui, &gate.gate_type, gate.position, tint);
        }
    }

    fn draw_gate_svg_with_pins(ui: &mut Ui, gate_type: &LogicGate, pos: Pos2, _tint: Color32) {
        let graphics = match gate_type {
            LogicGate::Nand => &NAND_GRAPHICS,
        };

        let size = Vec2::new(48.0, 36.0);
        let rect = egui::Rect::from_center_size(pos, size);
        let image = egui::Image::new(graphics.svg.clone()).fit_to_exact_size(rect.size());
        ui.put(rect, image);

        for pin in graphics.pins {
            let pin_pos = pos + pin.offset;
            let color = match pin.kind {
                PinKind::Input => Color32::RED,
                PinKind::Output => Color32::GREEN,
            };
            ui.painter().circle_filled(pin_pos, 4.0, color);
        }
    }
}

impl eframe::App for TemplateApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                let is_web = cfg!(target_arch = "wasm32");
                if !is_web {
                    ui.menu_button("File", |ui| {
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.add_space(16.0);
                }

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.main_layout(ui);
        });
    }
}
