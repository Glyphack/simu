use egui::{
    Align, Button, Color32, Image, ImageSource, Layout, Pos2, Rect, Sense, Ui, Vec2, Widget,
    include_image, vec2,
};

pub struct GateGraphics {
    // TODO: Figure out what is the correct way to deal with images
    pub svg: ImageSource<'static>,
    pub pins: &'static [PinInfo],
}

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

pub static WIRE_GRAPHICS: GateGraphics = GateGraphics {
    svg: include_image!("../assets/nand.svg"),
    pins: &[],
};

pub static NAND_GRAPHICS: GateGraphics = GateGraphics {
    svg: include_image!("../assets/nand.svg"),
    // TODO: offset must be made from the base_gate_size otherwise it will be unaligned when gates resize
    pins: &[
        PinInfo {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, -14.5),
        },
        PinInfo {
            kind: PinKind::Input,
            offset: Vec2::new(-37.0, 14.5),
        },
        PinInfo {
            kind: PinKind::Output,
            offset: Vec2::new(40.0, 0.2),
        },
    ],
};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub enum InstanceType {
    Nand,
    Wire,
}

impl InstanceType {
    fn graphics(&self) -> &GateGraphics {
        match self {
            InstanceType::Nand => &NAND_GRAPHICS,
            InstanceType::Wire => &WIRE_GRAPHICS,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, Default, Eq, PartialEq)]
pub enum Direction {
    Up,
    #[default]
    Right,
    Down,
    Left,
}

impl Direction {
    fn rotate_cw(self) -> Self {
        match self {
            Direction::Up => Direction::Right,
            Direction::Right => Direction::Down,
            Direction::Down => Direction::Left,
            Direction::Left => Direction::Up,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct GateInstance {
    pub id: InstanceId,
    pub gate_type: InstanceType,
    pub position: Pos2,
    pub direction: Direction,
}

impl GateInstance {}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct WireInstance {
    pub id: InstanceId,
    pub instance_type: InstanceType,
    pub position: Pos2,
    pub direction: Direction,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct CanvasConfig {
    pub base_gate_size: Vec2,
    pub base_pin_size: f32,
    pub base_input_pin_color: Color32,
    pub base_output_pin_color: Color32,
}

impl Default for CanvasConfig {
    fn default() -> Self {
        Self {
            base_gate_size: vec2(85.0, 75.0),
            base_pin_size: 4.5,
            base_input_pin_color: Color32::RED,
            base_output_pin_color: Color32::GREEN,
        }
    }
}

#[derive(
    serde::Deserialize,
    serde::Serialize,
    Default,
    Debug,
    Clone,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    Copy,
)]
pub struct InstanceId(u32);

impl InstanceId {
    pub fn incr(&mut self) {
        self.0 += 1;
    }
}

impl From<u32> for InstanceId {
    fn from(v: u32) -> Self {
        InstanceId(v)
    }
}

impl Into<u32> for InstanceId {
    fn into(self) -> u32 {
        self.0
    }
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
#[serde(default)]
pub struct TemplateApp {
    /// State
    ///
    /// Gates placed on the canvas
    gates: Vec<GateInstance>,
    /// Next unique ID for gates
    next_instance_id: InstanceId,

    /// Panel
    /// Dragging from panel
    panel_dragged_gate: Option<InstanceType>,
    /// Temporary drag position if dragging palette-gate
    panel_drag_position: Option<Pos2>,
    /// Orientation while dragging needed to allow rotate while dragging
    panel_drag_direction: Direction,

    /// Currently dragged gate id from canvas
    dragged_canvas_gate: Option<InstanceId>,
    /// Offset from mouse pointer to gate center at drag start
    drag_offset: Option<Vec2>,

    /// Config
    canvas_config: CanvasConfig,
}

impl TemplateApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Register all supported image loaders
        egui_extras::install_image_loaders(&cc.egui_ctx);
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
    }

    pub fn main_layout(&mut self, ui: &mut Ui) {
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            self.canvas_config = CanvasConfig::default();
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
        let nand_resp = ui.add(egui::ImageButton::new(image).sense(Sense::click_and_drag()));

        if nand_resp.drag_started() {
            self.panel_dragged_gate = Some(InstanceType::Nand);
            self.panel_drag_position = None;
            self.panel_drag_direction = Direction::Right;
        }
        if nand_resp.dragged() {
            if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
                self.panel_drag_position = Some(pointer_pos);
            }
        }

        ui.add_space(8.0);

        let wire_resp = ui.add(
            Button::new("Wire")
                .sense(Sense::click_and_drag())
                .min_size(vec2(48.0, 30.0)),
        );
        if wire_resp.drag_started() {
            self.panel_dragged_gate = Some(InstanceType::Wire);
            self.panel_drag_position = None;
            self.panel_drag_direction = Direction::Right;
        }
        if wire_resp.dragged() {
            if let Some(pointer_pos) = ui.ctx().pointer_interact_pos() {
                self.panel_drag_position = Some(pointer_pos);
            }
        }

        ui.add_space(8.0);

        if Button::new("Clear Canvas")
            .min_size(vec2(48.0, 30.0))
            .ui(ui)
            .clicked()
        {
            self.gates.clear();
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
                self.draw_instance(
                    ui,
                    gate_type,
                    pos,
                    self.panel_drag_direction,
                    Color32::YELLOW,
                );
            }
        }

        // spawn a new gate
        if let (Some(gate_type), Some(pos)) = (&self.panel_dragged_gate, self.panel_drag_position) {
            if rect.contains(pos) && ui.ctx().input(|i| i.pointer.any_released()) {
                self.gates.push(GateInstance {
                    id: self.next_instance_id,
                    gate_type: gate_type.clone(),
                    position: pos,
                    direction: self.panel_drag_direction,
                });
                self.next_instance_id.incr();
                self.panel_dragged_gate = None;
                self.panel_drag_position = None;
            }
        }

        let pointer_pos = ui.input(|i| i.pointer.interact_pos());
        let pointer_pressed = ui.input(|i| i.pointer.primary_down());
        let pointer_any_up = ui.input(|i| i.pointer.any_released());
        let r_pressed = ui.input(|i| i.key_pressed(egui::Key::R));

        if self.panel_dragged_gate.is_none()
            && self.dragged_canvas_gate.is_none()
            && pointer_pressed
        {
            if let Some(mouse_pos) = pointer_pos {
                for gate in self.gates.iter() {
                    let size = self.canvas_config.base_gate_size;
                    let gate_rect = egui::Rect::from_center_size(gate.position, size);
                    if gate_rect.contains(mouse_pos) {
                        self.dragged_canvas_gate = Some(gate.id.into());
                        self.drag_offset = Some(gate.position.to_vec2() - mouse_pos.to_vec2());
                        break;
                    }
                }
            }
        }
        if let (Some(drag_id), Some(mouse_pos), Some(offset)) =
            (self.dragged_canvas_gate, pointer_pos, self.drag_offset)
        {
            for gate in &mut self.gates {
                if gate.id == drag_id {
                    let mut new_pos = mouse_pos + offset;
                    new_pos.x = new_pos.x.clamp(rect.left() + 24.0, rect.right() - 24.0);
                    new_pos.y = new_pos.y.clamp(rect.top() + 18.0, rect.bottom() - 18.0);
                    gate.position = new_pos;
                    if r_pressed {
                        if let InstanceType::Wire = gate.gate_type {
                            gate.direction = gate.direction.rotate_cw();
                        }
                    }
                }
            }
            if pointer_any_up {
                self.dragged_canvas_gate = None;
                self.drag_offset = None;
            }
        }

        // Rotate hovered instance
        if r_pressed && self.dragged_canvas_gate.is_none() && self.panel_dragged_gate.is_none() {
            if let Some(mouse_pos) = pointer_pos {
                for gate in &mut self.gates {
                    if let InstanceType::Wire = gate.gate_type {
                        let size = self.canvas_config.base_gate_size;
                        let gate_rect = egui::Rect::from_center_size(gate.position, size);
                        if gate_rect.contains(mouse_pos) {
                            gate.direction = gate.direction.rotate_cw();
                            break;
                        }
                    }
                }
            }
        }

        // Rotate while dragging from panel
        if r_pressed {
            self.panel_drag_direction = self.panel_drag_direction.rotate_cw();
        }

        for gate in &self.gates {
            let highlight = self.dragged_canvas_gate == Some(gate.id);
            let tint = if highlight {
                Color32::YELLOW
            } else {
                Color32::LIGHT_BLUE
            };
            self.draw_instance(ui, &gate.gate_type, gate.position, gate.direction, tint);
        }
    }

    fn draw_instance(
        &self,
        ui: &mut Ui,
        gate: &InstanceType,
        pos: Pos2,
        orientation: Direction,
        _tint: Color32,
    ) {
        match gate {
            InstanceType::Wire => {
                let length = 40.0;
                let thickness = 6.0;
                let size = match orientation {
                    Direction::Left | Direction::Right => vec2(length, thickness),
                    Direction::Up | Direction::Down => vec2(thickness, length),
                };
                let rect = Rect::from_center_size(pos, size);
                ui.painter().rect_filled(rect, 1.0, Color32::from_gray(160));
            }
            InstanceType::Nand => {
                let rect = Rect::from_center_size(pos, self.canvas_config.base_gate_size);
                let image = Image::new(gate.graphics().svg.clone()).fit_to_exact_size(rect.size());
                ui.put(rect, image);

                for pin in gate.graphics().pins {
                    let pin_pos = pos + pin.offset;
                    let color = match pin.kind {
                        PinKind::Input => self.canvas_config.base_input_pin_color,
                        PinKind::Output => self.canvas_config.base_output_pin_color,
                    };
                    ui.painter()
                        .circle_filled(pin_pos, self.canvas_config.base_pin_size, color);
                }
            }
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
