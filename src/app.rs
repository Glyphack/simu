use std::{collections::HashMap, hash::Hash, usize};

use egui::{
    Align, Align2, Button, Color32, FontId, Image, Label, Layout, Pos2, Rect, Sense, Shape, Stroke,
    TextEdit, Ui, Vec2, Widget, pos2, vec2,
};

use crate::{assets, config::CanvasConfig};

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

/// All possible things that can appear on the screen.
#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Instance {
    id: InstanceId,
    ty: InstanceType,
    pos: Pos2,
    dir: Direction,
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub enum InstanceType {
    Nand,
    Wire,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct GateInstance {
    kind: GateKind,
}

impl GateKind {
    fn graphics(&self) -> &assets::InstanceGraphics {
        match self {
            Self::Nand => &assets::NAND_GRAPHICS,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub enum GateKind {
    Nand,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct WireInstance {
    pub start: Pos2,
    pub end: Pos2,
}

impl WireInstance {
    pub fn from_point(p: Pos2) -> WireInstance {
        return Self {
            start: p,
            end: pos2(p.x + 30.0, p.y),
        };
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

    pub fn usize(&self) -> usize {
        self.0 as usize
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

impl Into<usize> for InstanceId {
    fn into(self) -> usize {
        self.0 as usize
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct Resize {
    pub id: InstanceId,
    pub start: bool,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct PanelDrag {
    ty: InstanceType,
    /// Temporary drag position
    pos: Pos2,
    /// Orientation while dragging needed to allow rotate while dragging
    dir: Direction,
}

impl PanelDrag {
    fn new(ty: InstanceType, pos: Pos2) -> Self {
        Self {
            ty,
            pos,
            dir: Direction::Right,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
pub struct CanvasDrag {
    id: InstanceId,
    /// Offset from mouse pointer to gate center at drag start
    offset: Vec2,
}

impl CanvasDrag {
    fn new(id: InstanceId, offset: Vec2) -> Self {
        Self { id, offset }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct TemplateApp {
    /// State
    ///
    instances: Vec<Instance>,
    gates: HashMap<InstanceId, GateInstance>,
    wires: HashMap<InstanceId, WireInstance>,
    /// Next unique ID for gates
    next_instance_id: InstanceId,

    /// If a drag from panel is happening
    panel_drag: Option<PanelDrag>,

    /// Currently dragged gate id from canvas
    canvas_drag: Option<CanvasDrag>,

    /// An item is being resized
    resize: Option<Resize>,

    /// Config
    canvas_config: CanvasConfig,
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            instances: Default::default(),
            gates: Default::default(),
            wires: Default::default(),
            next_instance_id: InstanceId(0),
            panel_drag: None,
            canvas_drag: None,
            resize: None,
            canvas_config: CanvasConfig::default(),
        }
    }
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
            // TODO: need a better debugging method
            ui.add(egui::TextEdit::multiline(&mut format!(
                "world {:#?}",
                self.instances
            )));
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
        let image = egui::Image::new(GateKind::Nand.graphics().svg.clone()).max_height(70.0);
        let nand_resp = ui.add(egui::ImageButton::new(image).sense(Sense::click_and_drag()));

        if nand_resp.dragged()
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            self.panel_drag = Some(PanelDrag::new(InstanceType::Nand, pos))
        }

        ui.add_space(8.0);

        let wire_resp = ui.add(
            Button::new("Wire")
                .sense(Sense::click_and_drag())
                .min_size(vec2(48.0, 30.0)),
        );
        if wire_resp.dragged()
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            self.panel_drag = Some(PanelDrag::new(InstanceType::Wire, pos));
        }

        ui.add_space(8.0);

        if Button::new("Clear Canvas")
            .min_size(vec2(48.0, 30.0))
            .ui(ui)
            .clicked()
        {
            self.instances.clear();
            self.gates.clear();
            self.wires.clear();
            self.canvas_drag = None;
            self.panel_drag = None;
            self.resize = None;
            self.next_instance_id = InstanceId(0);
        }
    }

    fn draw_canvas(&mut self, ui: &mut Ui) {
        let (resp, _painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
        let canvas_rect = resp.rect;

        // handle dragging from panel
        if let Some(panel_drag) = &self.panel_drag {
            if canvas_rect.contains(panel_drag.pos) {
                match panel_drag.ty {
                    InstanceType::Nand => {
                        self.draw_gate(
                            ui,
                            &GateInstance {
                                kind: GateKind::Nand,
                            },
                            panel_drag.pos,
                        );
                    }
                    InstanceType::Wire => {
                        self.draw_wire(
                            ui,
                            &WireInstance::from_point(panel_drag.pos),
                            panel_drag.pos,
                            panel_drag.dir,
                        );
                    }
                }
            }
        }
        let mouse_up = ui.input(|i| i.pointer.any_released());
        if mouse_up {
            self.resize = None;
            self.canvas_drag = None;
        }

        // spawn a new gate
        if let Some(panel_drag) = &self.panel_drag
            && mouse_up
        {
            if canvas_rect.contains(panel_drag.pos) {
                match panel_drag.ty {
                    InstanceType::Nand => {
                        self.instances.push(Instance {
                            id: self.next_instance_id,
                            ty: InstanceType::Nand,
                            pos: panel_drag.pos,
                            dir: panel_drag.dir,
                        });
                        self.gates.insert(
                            self.next_instance_id,
                            GateInstance {
                                kind: GateKind::Nand,
                            },
                        );
                    }
                    InstanceType::Wire => {
                        self.instances.push(Instance {
                            id: self.next_instance_id,
                            ty: InstanceType::Wire,
                            pos: panel_drag.pos,
                            dir: panel_drag.dir,
                        });
                        self.wires.insert(
                            self.next_instance_id,
                            WireInstance::from_point(panel_drag.pos),
                        );
                    }
                }
                self.next_instance_id.incr();
                self.panel_drag = None;
            }
        }
        let pointer_pos = ui.input(|i| i.pointer.interact_pos());
        let pointer_pressed = ui.input(|i| i.pointer.primary_down());

        if pointer_pressed
            && let Some(mouse_pos) = pointer_pos
            && self.panel_drag.is_none()
            && self.canvas_drag.is_none()
            && self.resize.is_none()
        {
            let i = self.interacted_instance(mouse_pos);
            if let Some(instance) = i {
                match instance.ty {
                    InstanceType::Nand => {
                        self.canvas_drag = Some(CanvasDrag::new(
                            instance.id,
                            instance.pos.to_vec2() - mouse_pos.to_vec2(),
                        ));
                    }
                    InstanceType::Wire => {
                        let wire = self.get_wire(instance.id);
                        if mouse_pos.distance(wire.end) < 5.0 {
                            self.resize = Some(Resize {
                                id: instance.id,
                                start: false,
                            });
                        } else if mouse_pos.distance(wire.start) < 5.0 {
                            self.resize = Some(Resize {
                                id: instance.id,
                                start: true,
                            });
                        } else {
                            self.canvas_drag = Some(CanvasDrag::new(
                                instance.id,
                                wire.end.to_vec2() - mouse_pos.to_vec2(),
                            ));
                        }
                    }
                }
            }
        }

        // dragging on canvas
        if let (Some(id), Some(mouse_pos), Some(offset)) = {
            let id_opt = self.canvas_drag.as_ref().map(|c| c.id);
            let offset = self.canvas_drag.as_ref().map(|c| c.offset);
            (id_opt, pointer_pos, offset)
        } {
            let instance = self.get_instance(id);
            match instance.ty {
                InstanceType::Nand => {
                    let mut new_pos = mouse_pos + offset;
                    new_pos.x = new_pos
                        .x
                        .clamp(canvas_rect.left() + 24.0, canvas_rect.right() - 24.0);
                    new_pos.y = new_pos
                        .y
                        .clamp(canvas_rect.top() + 18.0, canvas_rect.bottom() - 18.0);
                    let instance = self.get_instance_mut(instance.id);
                    instance.pos = new_pos;
                }
                InstanceType::Wire => {
                    let wire = self.get_wire_mut(instance.id);
                    // TODO: Fix clamping for end and start when dragging one side can go out.
                    let mut new_pos = mouse_pos + offset;
                    new_pos.x = new_pos
                        .x
                        .clamp(canvas_rect.left() + 24.0, canvas_rect.right() - 24.0);
                    new_pos.y = new_pos
                        .y
                        .clamp(canvas_rect.top() + 18.0, canvas_rect.bottom() - 18.0);
                    let diff = new_pos - wire.end;
                    wire.end = new_pos;
                    wire.start += diff;
                }
            }
        }

        // expanding
        if let (Some(id), Some(mouse_pos), Some(start)) = {
            let id = self.resize.as_ref().map(|c| c.id);
            let start = self.resize.as_ref().map(|c| c.start);
            (id, pointer_pos, start)
        } {
            let wire = self.get_wire_mut(id);
            if start {
                wire.start = mouse_pos;
            } else {
                wire.end = mouse_pos;
            }
        }

        let r_pressed = ui.input(|i| i.key_pressed(egui::Key::R));
        if r_pressed {
            if let Some(c_drag) = &self.canvas_drag {
                let instance_r = self.get_instance_mut(c_drag.id);
                instance_r.dir = instance_r.dir.rotate_cw();
            }
            if let Some(p_drag) = &mut self.panel_drag {
                p_drag.dir = p_drag.dir.rotate_cw();
            }
            if let Some(mouse_pos) = pointer_pos {
                if let Some(i) = self.interacted_instance(mouse_pos) {
                    let wire = self.get_wire_mut(i.id);
                    let origin = (wire.start.to_vec2() + wire.end.to_vec2()) / 2.0;
                    // 0 -1
                    // 1 0
                    wire.start =
                        rotate_point(wire.start, origin.to_pos2(), std::f32::consts::FRAC_PI_2);
                    wire.end =
                        rotate_point(wire.end, origin.to_pos2(), std::f32::consts::FRAC_PI_2);
                }
            }
        }
        for instance in self.instances.iter() {
            match instance.ty {
                InstanceType::Nand => {
                    let gate = self.get_gate(instance.id);
                    self.draw_gate(ui, gate, instance.pos);
                }
                InstanceType::Wire => {
                    let wire = self.get_wire(instance.id);
                    self.draw_wire(ui, wire, instance.pos, instance.dir);
                }
            }
        }
    }

    fn draw_wire(&self, ui: &mut Ui, wire: &WireInstance, pos: Pos2, dir: Direction) {
        let length = 40.0;
        let thickness = 6.0;
        ui.painter().line_segment(
            [wire.start, wire.end],
            Stroke::new(thickness, Color32::LIGHT_RED),
        );
    }

    fn draw_gate(&self, ui: &mut Ui, gate: &GateInstance, pos: Pos2) {
        let rect = Rect::from_center_size(pos, self.canvas_config.base_gate_size);
        let image = Image::new(gate.kind.graphics().svg.clone()).fit_to_exact_size(rect.size());
        ui.put(rect, image);

        for pin in gate.kind.graphics().pins {
            let pin_pos = pos + pin.offset;
            let color = match pin.kind {
                assets::PinKind::Input => self.canvas_config.base_input_pin_color,
                assets::PinKind::Output => self.canvas_config.base_output_pin_color,
            };
            ui.painter()
                .circle_filled(pin_pos, self.canvas_config.base_pin_size, color);
        }
    }

    fn interacted_instance(&self, mouse_pos: Pos2) -> Option<&Instance> {
        let mut i: Option<&Instance> = None;
        for instance in self.instances.iter() {
            match instance.ty {
                InstanceType::Nand => {
                    let size = self.canvas_config.base_gate_size;
                    let gate_rect = egui::Rect::from_center_size(instance.pos, size);
                    if gate_rect.contains(mouse_pos) {
                        i = Some(instance);
                        break;
                    }
                }
                InstanceType::Wire => {
                    let wire = self.get_wire(instance.id);
                    let dist = distance_point_to_segment(mouse_pos, wire.start, wire.end);
                    if dist < 10.0 {
                        i = Some(instance);
                        break;
                    }
                }
            }
        }
        i
    }
}

impl TemplateApp {
    fn get_wire(&self, id: InstanceId) -> &WireInstance {
        self.wires.get(&id).expect("should not happen")
    }
    fn get_wire_mut(&mut self, id: InstanceId) -> &mut WireInstance {
        self.wires.get_mut(&id).expect("should not happen")
    }

    fn get_gate(&self, id: InstanceId) -> &GateInstance {
        self.gates.get(&id).expect("should not happen")
    }

    fn get_instance(&self, id: InstanceId) -> &Instance {
        self.instances.get(id.usize()).expect("should not happen")
    }

    fn get_instance_mut(&mut self, id: InstanceId) -> &mut Instance {
        self.instances
            .get_mut(id.usize())
            .expect("should not happen")
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

pub fn distance_point_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab: Vec2 = b - a;
    let ap: Vec2 = p - a;

    let ab_len2 = ab.x * ab.x + ab.y * ab.y;
    if ab_len2 == 0.0 {
        return (p - a).length();
    }

    let t = ((ap.x * ab.x + ap.y * ab.y) / ab_len2).clamp(0.0, 1.0);

    let closest = a + ab * t;
    (p - closest).length()
}

fn rotate_point(point: Pos2, origin: Pos2, angle: f32) -> Pos2 {
    let s = angle.sin();
    let c = angle.cos();

    let px = point.x - origin.x;
    let py = point.y - origin.y;

    let xnew = px * c - py * s;
    let ynew = px * s + py * c;

    Pos2::new(xnew + origin.x, ynew + origin.y)
}
