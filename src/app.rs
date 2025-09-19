use std::collections::HashSet;
use std::fmt::Write as _;
use std::hash::Hash;

use egui::{
    Align, Button, Color32, CornerRadius, Image, Layout, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
    Vec2, Widget as _, vec2,
};
use slotmap::{Key as _, SecondaryMap, SlotMap};

use crate::{assets, config::CanvasConfig, drag::Drag};

pub const EDGE_THRESHOLD: f32 = 10.0;
pub const WIRE_HIT_DISTANCE: f32 = 10.0;

pub const COLOR_PIN_DETACH_HINT: Color32 = Color32::RED;
pub const COLOR_PIN_POWERED_OUTLINE: Color32 = Color32::BLUE;
pub const COLOR_WIRE_POWERED: Color32 = Color32::GREEN;
pub const COLOR_WIRE_IDLE: Color32 = Color32::LIGHT_BLUE;
pub const COLOR_WIRE_HOVER: Color32 = Color32::GRAY;
pub const COLOR_HOVER_OUTLINE: Color32 = Color32::GRAY;
pub const COLOR_ENDPOINT_HOVER: Color32 = Color32::LIGHT_YELLOW;
pub const COLOR_POTENTIAL_CONN_HIGHLIGHT: Color32 = Color32::LIGHT_YELLOW;
pub const COLOR_SELECTION_HIGHLIGHT: Color32 = Color32::GRAY;
pub const COLOR_SELECTION_BOX: Color32 = Color32::LIGHT_BLUE;

slotmap::new_key_type! {
    pub struct InstanceId;
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub enum InstancePosOffset {
    Gate(GateKind, Vec2),
    Power(Vec2),
    Wire(Vec2, Vec2),
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub enum InstanceKind {
    Gate(GateKind),
    Power,
    Wire,
}

// A specific pin on an instance
#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone, Eq, PartialEq, Hash)]
pub struct Pin {
    pub ins: InstanceId,
    pub index: u32,
}

// A normalized, order-independent connection between two pins
#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone, Eq)]
pub struct Connection {
    pub a: Pin,
    pub b: Pin,
}

impl Connection {
    pub fn new(p1: Pin, p2: Pin) -> Self {
        // Normalize by ordering on (ins, index)
        if (p2.ins.data(), p2.index) < (p1.ins.data(), p1.index) {
            Self { a: p2, b: p1 }
        } else {
            Self { a: p1, b: p2 }
        }
    }

    pub fn involves_instance(&self, id: InstanceId) -> bool {
        self.a.ins == id || self.b.ins == id
    }

    fn get_pin_first(&self, pin: Pin) -> Option<(Pin, Pin)> {
        if self.a == pin {
            Some((self.a, self.b))
        } else if self.b == pin {
            Some((self.b, self.a))
        } else {
            None
        }
    }
}

impl PartialEq for Connection {
    fn eq(&self, other: &Self) -> bool {
        self.a == other.a && self.b == other.b
    }
}

impl Hash for Connection {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.a.hash(state);
        self.b.hash(state);
    }
}

// Gate

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Gate {
    pub kind: GateKind,
    // Center position
    pub pos: Pos2,
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub enum GateKind {
    And,
    Nand,
    Or,
    Nor,
    Xor,
    Xnor,
}

impl GateKind {
    pub fn graphics(&self) -> &assets::InstanceGraphics {
        match self {
            Self::Nand => &assets::NAND_GRAPHICS,
            Self::And => &assets::AND_GRAPHICS,
            Self::Or => &assets::OR_GRAPHICS,
            Self::Nor => &assets::NOR_GRAPHICS,
            Self::Xor => &assets::XOR_GRAPHICS,
            Self::Xnor => &assets::XNOR_GRAPHICS,
        }
    }
}

// Gate end

// Power

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Power {
    // Center position
    pub pos: Pos2,
    pub on: bool,
}

impl Power {
    fn graphics(&self) -> &assets::InstanceGraphics {
        if self.on {
            &assets::POWER_ON_GRAPHICS
        } else {
            &assets::POWER_OFF_GRAPHICS
        }
    }
}

// Power end

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Wire {
    pub start: Pos2,
    pub end: Pos2,
}

impl Wire {}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct DB {
    // Primary key allocator; ensures unique keys across all instance kinds
    pub instances: SlotMap<InstanceId, ()>,
    // Type registry for each instance id
    pub types: SecondaryMap<InstanceId, InstanceKind>,
    // Per-kind payloads keyed off the primary key space
    pub gates: SecondaryMap<InstanceId, Gate>,
    pub powers: SecondaryMap<InstanceId, Power>,
    pub wires: SecondaryMap<InstanceId, Wire>,
    pub connections: HashSet<Connection>,
}

impl DB {
    pub fn new_gate(&mut self, g: Gate) -> InstanceId {
        let k = self.instances.insert(());
        self.gates.insert(k, g);
        let kind = self
            .gates
            .get(k)
            .expect("gate must exist right after insertion")
            .kind;
        self.types.insert(k, InstanceKind::Gate(kind));
        k
    }

    pub fn new_power(&mut self, p: Power) -> InstanceId {
        let k = self.instances.insert(());
        self.powers.insert(k, p);
        self.types.insert(k, InstanceKind::Power);
        k
    }

    pub fn new_wire(&mut self, w: Wire) -> InstanceId {
        let k = self.instances.insert(());
        self.wires.insert(k, w);
        self.types.insert(k, InstanceKind::Wire);
        k
    }

    pub fn ty(&self, id: InstanceId) -> InstanceKind {
        self.types
            .get(id)
            .copied()
            .expect("instance type missing for id")
    }

    pub fn get_gate(&self, id: InstanceId) -> &Gate {
        self.gates.get(id).expect("gate not found")
    }

    pub fn get_gate_mut(&mut self, id: InstanceId) -> &mut Gate {
        self.gates.get_mut(id).expect("gate not found (mut)")
    }

    pub fn get_power(&self, id: InstanceId) -> &Power {
        self.powers.get(id).expect("power not found")
    }

    pub fn get_power_mut(&mut self, id: InstanceId) -> &mut Power {
        self.powers.get_mut(id).expect("power not found (mut)")
    }

    pub fn get_wire(&self, id: InstanceId) -> &Wire {
        self.wires.get(id).expect("wire not found")
    }

    pub fn get_wire_mut(&mut self, id: InstanceId) -> &mut Wire {
        self.wires.get_mut(id).expect("wire not found (mut)")
    }

    pub fn pins_of(&self, id: InstanceId) -> Vec<Pin> {
        match self.ty(id) {
            InstanceKind::Gate(gk) => {
                let n = gk.graphics().pins.len();
                (0..n as u32).map(|i| Pin { ins: id, index: i }).collect()
            }
            InstanceKind::Power => {
                // power graphics depends on on/off, but pin layout is identical
                let n = assets::POWER_ON_GRAPHICS.pins.len();
                (0..n as u32).map(|i| Pin { ins: id, index: i }).collect()
            }
            InstanceKind::Wire => vec![Pin { ins: id, index: 0 }, Pin { ins: id, index: 1 }],
        }
    }

    pub fn pin_position(&self, pin: Pin) -> Pos2 {
        match self.ty(pin.ins) {
            InstanceKind::Gate(gk) => {
                let g = self.get_gate(pin.ins);
                let info = gk.graphics().pins[pin.index as usize];
                g.pos + info.offset
            }
            InstanceKind::Power => {
                let p = self.get_power(pin.ins);
                let info = p.graphics().pins[pin.index as usize];
                p.pos + info.offset
            }
            InstanceKind::Wire => {
                let w = self.get_wire(pin.ins);
                if pin.index == 0 { w.start } else { w.end }
            }
        }
    }

    pub fn connected_pins_of_instance(&self, id: InstanceId) -> Vec<Pin> {
        let mut out = Vec::new();
        for c in &self.connections {
            if c.a.ins == id {
                out.push(c.b);
            } else if c.b.ins == id {
                out.push(c.a);
            }
        }
        out
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct App {
    pub canvas_config: CanvasConfig,
    pub drag: Option<Drag>,
    pub hovered: Option<InstanceId>,
    pub db: DB,
    // possible connections while dragging
    pub potential_connections: HashSet<Connection>,
    // energized pins based on current simulation
    pub current: HashSet<Pin>,
    // mark when current needs recomputation
    pub current_dirty: bool,
    pub show_debug: bool,
    // selection set and move preview
    pub selected: std::collections::HashSet<InstanceId>,
    //Copied. Items with their offset compared to a middle point in the rectangle
    pub clipboard: Vec<InstancePosOffset>,
}

impl Default for App {
    fn default() -> Self {
        Self {
            db: DB {
                instances: Default::default(),
                types: Default::default(),
                gates: Default::default(),
                powers: Default::default(),
                wires: Default::default(),
                connections: Default::default(),
            },
            canvas_config: Default::default(),
            drag: Default::default(),
            hovered: Default::default(),
            potential_connections: Default::default(),
            current: Default::default(),
            current_dirty: true,
            show_debug: true,
            selected: Default::default(),
            clipboard: Default::default(),
        }
    }
}

impl eframe::App for App {
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

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_debug, "World Debug");
                });
                ui.add_space(16.0);

                egui::widgets::global_theme_preference_buttons(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.main_layout(ui);
        });
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        }
    }

    pub fn main_layout(&mut self, ui: &mut Ui) {
        if self.show_debug {
            egui::Window::new("Debug logs").show(ui.ctx(), |ui| {
                egui_logger::logger_ui().show(ui);
            });
        }
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            self.canvas_config = CanvasConfig::default();

            // World Debugger: show database and world state (full height)
            if self.show_debug {
                // Let the debug panel use the full available height
                let full_h = ui.available_height();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut dbg = self.debug_string();
                    ui.add_sized(vec2(320.0, full_h), egui::TextEdit::multiline(&mut dbg));
                });
            }

            ui.vertical(|ui| {
                ui.heading("Logic Gates");
                self.draw_panel(ui);
            });
            ui.separator();
            ui.vertical(|ui| {
                ui.heading("Canvas");
                ui.label("press d to remove object");
                self.draw_canvas(ui);
            });
        });
    }

    fn debug_string(&self) -> String {
        let mut out = String::new();
        writeln!(
            out,
            "counts: gates={}, powers={}, wires={}, conns={}",
            self.db.gates.len(),
            self.db.powers.len(),
            self.db.wires.len(),
            self.db.connections.len()
        )
        .ok();
        writeln!(out, "hovered: {:?}", self.hovered).ok();
        writeln!(out, "drag: {:?}", self.drag).ok();
        writeln!(out, "potential_conns: {}", self.potential_connections.len()).ok();
        writeln!(out, "clipboard: {:?}", self.clipboard).ok();
        writeln!(out, "selected: {:?}", self.selected).ok();

        writeln!(out, "\nGates:").ok();
        for (id, g) in &self.db.gates {
            writeln!(
                out,
                "  {:?}: kind={:?} pos=({:.1},{:.1})",
                id, g.kind, g.pos.x, g.pos.y
            )
            .ok();
            // pins
            for (i, pin) in g.kind.graphics().pins.iter().enumerate() {
                let p = g.pos + pin.offset;
                writeln!(out, "    pin#{i} {:?} at ({:.1},{:.1})", pin.kind, p.x, p.y).ok();
            }
        }

        writeln!(out, "\nPowers:").ok();
        for (id, p) in &self.db.powers {
            writeln!(
                out,
                "  {:?}: on={} pos=({:.1},{:.1})",
                id, p.on, p.pos.x, p.pos.y
            )
            .ok();
            for (i, pin) in p.graphics().pins.iter().enumerate() {
                let pp = p.pos + pin.offset;
                writeln!(
                    out,
                    "    pin#{i} {:?} at ({:.1},{:.1})",
                    pin.kind, pp.x, pp.y
                )
                .ok();
            }
        }

        writeln!(out, "\nWires:").ok();
        for (id, w) in &self.db.wires {
            writeln!(
                out,
                "  {:?}: start=({:.1},{:.1}) end=({:.1},{:.1})",
                id, w.start.x, w.start.y, w.end.x, w.end.y
            )
            .ok();
        }

        writeln!(out, "\nConnections:").ok();
        for c in &self.db.connections {
            let p1 = self.db.pin_position(c.a);
            let p2 = self.db.pin_position(c.b);
            writeln!(
                out,
                "  ({:?}:{}) <-> ({:?}:{}) | ({:.1},{:.1})<->({:.1},{:.1})",
                c.a.ins, c.a.index, c.b.ins, c.b.index, p1.x, p1.y, p2.x, p2.y
            )
            .ok();
        }

        out
    }

    #[expect(clippy::too_many_lines)]
    fn draw_panel(&mut self, ui: &mut Ui) {
        let image = egui::Image::new(GateKind::Nand.graphics().svg.clone()).max_height(70.0);
        let nand_resp = ui.add(egui::ImageButton::new(image).sense(Sense::click_and_drag()));

        if nand_resp.dragged()
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            self.drag = Some(Drag::Panel {
                pos,
                kind: InstanceKind::Gate(GateKind::Nand),
            });
        }

        ui.add_space(8.0);

        let image = egui::Image::new(GateKind::And.graphics().svg.clone()).max_height(70.0);
        let and_resp = ui.add(egui::ImageButton::new(image).sense(Sense::click_and_drag()));

        if and_resp.dragged()
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            self.drag = Some(Drag::Panel {
                pos,
                kind: InstanceKind::Gate(GateKind::And),
            });
        }

        ui.add_space(8.0);

        let image = egui::Image::new(GateKind::Or.graphics().svg.clone()).max_height(70.0);
        let or_resp = ui.add(egui::ImageButton::new(image).sense(Sense::click_and_drag()));

        if or_resp.dragged()
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            self.drag = Some(Drag::Panel {
                pos,
                kind: InstanceKind::Gate(GateKind::Or),
            });
        }

        ui.add_space(8.0);

        let image = egui::Image::new(GateKind::Nor.graphics().svg.clone()).max_height(70.0);
        let nor_resp = ui.add(egui::ImageButton::new(image).sense(Sense::click_and_drag()));

        if nor_resp.dragged()
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            self.drag = Some(Drag::Panel {
                pos,
                kind: InstanceKind::Gate(GateKind::Nor),
            });
        }

        ui.add_space(8.0);

        let image = egui::Image::new(GateKind::Xor.graphics().svg.clone()).max_height(70.0);
        let xor_resp = ui.add(egui::ImageButton::new(image).sense(Sense::click_and_drag()));

        if xor_resp.dragged()
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            self.drag = Some(Drag::Panel {
                pos,
                kind: InstanceKind::Gate(GateKind::Xor),
            });
        }

        ui.add_space(8.0);

        let image = egui::Image::new(GateKind::Xnor.graphics().svg.clone()).max_height(70.0);
        let xnor_resp = ui.add(egui::ImageButton::new(image).sense(Sense::click_and_drag()));

        if xnor_resp.dragged()
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            self.drag = Some(Drag::Panel {
                pos,
                kind: InstanceKind::Gate(GateKind::Xnor),
            });
        }

        ui.add_space(8.0);

        let pwr_image = egui::Image::new(assets::POWER_ON_GRAPHICS.svg.clone()).max_height(70.0);
        let pwr_resp = ui.add(egui::ImageButton::new(pwr_image).sense(Sense::click_and_drag()));
        if pwr_resp.dragged()
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            self.drag = Some(Drag::Panel {
                pos,
                kind: InstanceKind::Power,
            });
        }

        ui.add_space(8.0);

        let wire_resp = ui.add(
            Button::new("Wire")
                .sense(Sense::click_and_drag())
                .min_size(vec2(78.0, 30.0)),
        );
        if wire_resp.dragged()
            && let Some(pos) = ui.ctx().pointer_interact_pos()
        {
            self.drag = Some(Drag::Panel {
                pos,
                kind: InstanceKind::Wire,
            });
        }

        ui.add_space(8.0);

        if Button::new("Clear Canvas")
            .min_size(vec2(48.0, 30.0))
            .ui(ui)
            .clicked()
        {
            self.db.gates.clear();
            self.db.powers.clear();
            self.db.wires.clear();
            self.db.types.clear();
            self.db.instances.clear();
            self.db.connections.clear();
            self.hovered = None;
            self.drag = None;
            self.current.clear();
            self.current_dirty = false;
        }
    }

    #[expect(clippy::too_many_lines)]
    fn draw_canvas(&mut self, ui: &mut Ui) {
        let (resp, _painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
        let canvas_rect = resp.rect;

        let mouse_up = ui.input(|i| i.pointer.any_released());
        let pointer_pressed = ui.input(|i| i.pointer.primary_down());
        let right_clicked = ui.input(|i| i.pointer.secondary_clicked());
        let mouse_pos = ui.ctx().pointer_interact_pos();

        let mut copy_event_detected = false;
        let mut paste_event_detected = false;
        ui.ctx().input(|i| {
            for event in &i.events {
                if matches!(event, egui::Event::Copy) {
                    log::info!("Copy detected");
                    copy_event_detected = true;
                }
                if let egui::Event::Paste(_) = event {
                    paste_event_detected = true;
                }
            }
        });

        let d_pressed = ui.input(|i| i.key_pressed(egui::Key::D));

        if d_pressed && let Some(id) = self.hovered.take() {
            self.delete_instance(id);
        }

        if copy_event_detected && !self.selected.is_empty() {
            let mut points = vec![];
            for &selected in &self.selected {
                match self.db.ty(selected) {
                    InstanceKind::Gate(_) => {
                        let g = self.db.get_gate(selected);
                        points.push(g.pos);
                    }
                    InstanceKind::Power => {
                        let p = self.db.get_power(selected);
                        points.push(p.pos);
                    }
                    InstanceKind::Wire => {
                        let w = self.db.get_wire(selected);
                        points.push(w.start);
                        points.push(w.end);
                    }
                }
            }
            let rect = Rect::from_points(&points);
            let center = rect.center();

            let mut object_pos = vec![];

            for &selected in &self.selected {
                let ty = self.db.ty(selected);
                match ty {
                    InstanceKind::Gate(kind) => {
                        let g = self.db.get_gate(selected);
                        object_pos.push(InstancePosOffset::Gate(kind, center - g.pos));
                    }
                    InstanceKind::Power => {
                        let p = self.db.get_power(selected);
                        object_pos.push(InstancePosOffset::Power(center - p.pos));
                    }
                    InstanceKind::Wire => {
                        let w = self.db.get_wire(selected);
                        object_pos.push(InstancePosOffset::Wire(center - w.start, center - w.end));
                    }
                }
            }

            self.clipboard = object_pos
        }

        if paste_event_detected
            && !self.clipboard.is_empty()
            && let Some(mouse) = mouse_pos
        {
            for to_paste in self.clipboard.clone() {
                let id = match to_paste {
                    InstancePosOffset::Gate(gate_kind, offset) => self.db.new_gate(Gate {
                        kind: gate_kind,
                        pos: mouse - offset,
                    }),
                    InstancePosOffset::Power(offset) => self.db.new_power(Power {
                        pos: mouse - offset,
                        on: false,
                    }),
                    InstancePosOffset::Wire(s, e) => self.db.new_wire(Wire {
                        start: mouse - s,
                        end: mouse - e,
                    }),
                };
                self.potential_connections = self.compute_potential_connections_for_instance(id);
                self.finalize_connections_for_instance(id, &canvas_rect);
            }
            self.selected.clear();
        }

        if let Some(mouse) = mouse_pos {
            self.handle_dragging(ui, mouse, &canvas_rect);
            self.hovered = self.interacted_instance(mouse);
            if pointer_pressed && canvas_rect.contains(mouse) {
                self.handle_drag_start_canvas(mouse);
            }
        }

        if right_clicked
            && let Some(id) = self.hovered
            && matches!(self.db.ty(id), InstanceKind::Power)
        {
            let p = self.db.get_power_mut(id);
            p.on = !p.on;
            self.current_dirty = true;
        }

        if mouse_up {
            self.handle_drag_end(&canvas_rect, mouse_pos);
        }

        if self.current_dirty {
            self.recompute_current();
        }

        for (id, gate) in &self.db.gates {
            self.draw_gate(ui, id, gate);
        }
        for (id, power) in &self.db.powers {
            self.draw_power(ui, id, power);
        }
        for (id, wire) in &self.db.wires {
            let has_current = self.current.contains(&Pin { ins: id, index: 0 });
            self.draw_wire(ui, *wire, self.hovered == Some(id), has_current);
        }

        if !self.potential_connections.is_empty() {
            for c in &self.potential_connections {
                // Highlight the pin belonging to the currently moving instance if any
                let pin_to_highlight = match self.drag {
                    Some(Drag::Canvas { id, .. }) => {
                        if c.a.ins == id {
                            c.a
                        } else if c.b.ins == id {
                            c.b
                        } else {
                            continue;
                        }
                    }
                    Some(Drag::Resize { id, start }) => {
                        // Only highlight the endpoint being resized
                        let target_pin = Pin {
                            ins: id,
                            index: u32::from(!start),
                        };
                        if c.a == target_pin {
                            c.a
                        } else if c.b == target_pin {
                            c.b
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };
                let p = self.db.pin_position(pin_to_highlight);
                ui.painter()
                    .circle_filled(p, EDGE_THRESHOLD, COLOR_POTENTIAL_CONN_HIGHLIGHT);
            }
        }

        self.highlight_hovered(ui);
        self.draw_selection_highlight(ui);
    }

    fn draw_gate(&self, ui: &mut Ui, id: InstanceId, gate: &Gate) {
        let rect = Rect::from_center_size(gate.pos, self.canvas_config.base_gate_size);
        let image = Image::new(gate.kind.graphics().svg.clone()).fit_to_exact_size(rect.size());
        ui.put(rect, image);

        for (i, pin) in gate.kind.graphics().pins.iter().enumerate() {
            let pin_pos = gate.pos + pin.offset;
            let color = match pin.kind {
                assets::PinKind::Input => self.canvas_config.base_input_pin_color,
                assets::PinKind::Output => self.canvas_config.base_output_pin_color,
            };
            ui.painter()
                .circle_filled(pin_pos, self.canvas_config.base_pin_size, color);

            if self.current.contains(&Pin {
                ins: id,
                index: i as u32,
            }) {
                ui.painter().circle_stroke(
                    pin_pos,
                    self.canvas_config.base_pin_size + 3.0,
                    Stroke::new(2.0, COLOR_PIN_POWERED_OUTLINE),
                );
            }
        }
    }

    pub fn draw_gate_preview(&self, ui: &mut Ui, gate_kind: GateKind, pos: Pos2) {
        let rect = Rect::from_center_size(pos, self.canvas_config.base_gate_size);
        let image = Image::new(gate_kind.graphics().svg.clone()).fit_to_exact_size(rect.size());
        ui.put(rect, image);

        for pin in gate_kind.graphics().pins {
            let pin_pos = pos + pin.offset;
            let color = match pin.kind {
                assets::PinKind::Input => self.canvas_config.base_input_pin_color,
                assets::PinKind::Output => self.canvas_config.base_output_pin_color,
            };
            ui.painter()
                .circle_filled(pin_pos, self.canvas_config.base_pin_size, color);
        }
    }

    fn draw_power(&self, ui: &mut Ui, id: InstanceId, power: &Power) {
        let rect = Rect::from_center_size(power.pos, self.canvas_config.base_gate_size);
        let image = Image::new(power.graphics().svg.clone()).fit_to_exact_size(rect.size());
        ui.put(rect, image);

        for (i, pin) in power.graphics().pins.iter().enumerate() {
            let pin_pos = power.pos + pin.offset;
            let color = match pin.kind {
                assets::PinKind::Input => self.canvas_config.base_input_pin_color,
                assets::PinKind::Output => self.canvas_config.base_output_pin_color,
            };
            ui.painter()
                .circle_filled(pin_pos, self.canvas_config.base_pin_size, color);

            if self.current.contains(&Pin {
                ins: id,
                index: i as u32,
            }) {
                ui.painter().circle_stroke(
                    pin_pos,
                    self.canvas_config.base_pin_size + 3.0,
                    Stroke::new(2.0, COLOR_PIN_POWERED_OUTLINE),
                );
            }
        }
    }

    pub fn draw_power_preview(&self, ui: &mut Ui, pos: Pos2) {
        let power = Power { pos, on: true };
        let rect = Rect::from_center_size(power.pos, self.canvas_config.base_gate_size);
        let image = Image::new(power.graphics().svg.clone()).fit_to_exact_size(rect.size());
        ui.put(rect, image);

        for pin in power.graphics().pins {
            let pin_pos = power.pos + pin.offset;
            let color = match pin.kind {
                assets::PinKind::Input => self.canvas_config.base_input_pin_color,
                assets::PinKind::Output => self.canvas_config.base_output_pin_color,
            };
            ui.painter()
                .circle_filled(pin_pos, self.canvas_config.base_pin_size, color);
        }
    }

    pub fn draw_wire(&self, ui: &Ui, mouse: Wire, hovered: bool, has_current: bool) {
        let mut color = if has_current {
            COLOR_WIRE_POWERED
        } else {
            COLOR_WIRE_IDLE
        };

        if hovered {
            color = COLOR_WIRE_HOVER;
        }

        ui.painter().line_segment(
            [mouse.start, mouse.end],
            Stroke::new(self.canvas_config.wire_thickness, color),
        );
    }

    fn highlight_hovered(&self, ui: &Ui) {
        let Some(hovered) = self.hovered else {
            return;
        };

        match self.db.ty(hovered) {
            InstanceKind::Gate(gate_kind) => {
                let gate = self.db.get_gate(hovered);
                let outer = Rect::from_center_size(
                    gate.pos,
                    self.canvas_config.base_gate_size + vec2(3.0, 3.0),
                );
                ui.painter().rect_stroke(
                    outer,
                    CornerRadius::default(),
                    Stroke::new(2.0, COLOR_HOVER_OUTLINE),
                    StrokeKind::Middle,
                );

                if let Some(mouse) = ui.ctx().pointer_interact_pos() {
                    for (i, pin_info) in gate_kind.graphics().pins.iter().enumerate() {
                        let pin = Pin {
                            ins: hovered,
                            index: i as u32,
                        };
                        let pin_pos = gate.pos + pin_info.offset;
                        if mouse.distance(pin_pos) < EDGE_THRESHOLD && self.is_pin_connected(pin) {
                            ui.painter().circle_filled(
                                pin_pos,
                                EDGE_THRESHOLD,
                                COLOR_PIN_DETACH_HINT,
                            );
                        }
                    }
                }
            }
            InstanceKind::Power => {
                let power = self.db.get_power(hovered);
                let outer = Rect::from_center_size(
                    power.pos,
                    self.canvas_config.base_gate_size + vec2(3.0, 3.0),
                );
                ui.painter().rect_stroke(
                    outer,
                    CornerRadius::default(),
                    Stroke::new(2.0, COLOR_HOVER_OUTLINE),
                    StrokeKind::Middle,
                );

                if let Some(mouse) = ui.ctx().pointer_interact_pos() {
                    for (i, pin_info) in power.graphics().pins.iter().enumerate() {
                        let pin = Pin {
                            ins: hovered,
                            index: i as u32,
                        };
                        let pin_pos = power.pos + pin_info.offset;
                        if mouse.distance(pin_pos) < EDGE_THRESHOLD && self.is_pin_connected(pin) {
                            ui.painter().circle_filled(
                                pin_pos,
                                EDGE_THRESHOLD,
                                COLOR_PIN_DETACH_HINT,
                            );
                        }
                    }
                }
            }
            InstanceKind::Wire => {
                let wire = self.db.get_wire(hovered);
                if let Some(mouse) = ui.ctx().pointer_interact_pos() {
                    if mouse.distance(wire.start) < EDGE_THRESHOLD {
                        ui.painter().circle_filled(
                            wire.start,
                            EDGE_THRESHOLD,
                            COLOR_ENDPOINT_HOVER,
                        );
                    } else if mouse.distance(wire.end) < EDGE_THRESHOLD {
                        ui.painter()
                            .circle_filled(wire.end, EDGE_THRESHOLD, COLOR_ENDPOINT_HOVER);
                    }
                }
            }
        }

        if let Some(Drag::Resize { id, start }) = self.drag {
            let w = self.db.get_wire(id);
            let p = if start { w.start } else { w.end };
            ui.painter()
                .circle_filled(p, EDGE_THRESHOLD, COLOR_ENDPOINT_HOVER);
        }
    }

    fn is_pin_connected(&self, pin: Pin) -> bool {
        self.db.connections.iter().any(|c| c.a == pin || c.b == pin)
    }

    fn draw_selection_highlight(&self, ui: &Ui) {
        for &id in &self.selected {
            match self.db.ty(id) {
                InstanceKind::Gate(_) => {
                    let g = self.db.get_gate(id);
                    let r = Rect::from_center_size(
                        g.pos,
                        self.canvas_config.base_gate_size + vec2(6.0, 6.0),
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(2.0, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
                InstanceKind::Power => {
                    let p = self.db.get_power(id);
                    let r = Rect::from_center_size(
                        p.pos,
                        self.canvas_config.base_gate_size + vec2(6.0, 6.0),
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(2.0, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
                InstanceKind::Wire => {}
            }
        }
    }

    // ---------- Simulation ----------

    fn recompute_current(&mut self) {
        let mut current: HashSet<Pin> = HashSet::new();
        let ids: Vec<InstanceId> = self.db.types.keys().collect();
        for id in ids {
            self.eval_instance(id, &mut current);
        }
        self.current = current;
        self.current_dirty = false;
    }

    fn eval_instance(&self, id: InstanceId, current: &mut HashSet<Pin>) {
        match self.db.ty(id) {
            InstanceKind::Power => {
                let p = self.db.get_power(id);
                if p.on {
                    current.insert(Pin { ins: id, index: 0 });
                }
            }
            InstanceKind::Wire => {
                let a = Pin { ins: id, index: 0 };
                let b = Pin { ins: id, index: 1 };
                let mut visiting = HashSet::new();
                let a_on = self.eval_pin(a, current, &mut visiting);
                visiting.clear();
                let b_on = self.eval_pin(b, current, &mut visiting);
                if a_on || b_on {
                    current.insert(a);
                    current.insert(b);
                }
            }
            InstanceKind::Gate(kind) => {
                // pins: 0=input A, 2=input B, 1=output
                let a = Pin { ins: id, index: 0 };
                let b = Pin { ins: id, index: 2 };
                let out = Pin { ins: id, index: 1 };
                let mut visiting = HashSet::new();
                let a_on = self.eval_pin(a, current, &mut visiting);
                visiting.clear();
                let b_on = self.eval_pin(b, current, &mut visiting);
                let out_on = match kind {
                    GateKind::And => a_on && b_on,
                    GateKind::Nand => !(a_on && b_on),
                    GateKind::Or => a_on || b_on,
                    GateKind::Nor => !(a_on || b_on),
                    GateKind::Xor => (a_on && !b_on) || (!a_on && b_on),
                    GateKind::Xnor => a_on == b_on,
                };
                if out_on {
                    current.insert(out);
                }
            }
        }
    }

    fn connected_pins(&self, pin: Pin) -> Vec<Pin> {
        let mut res = Vec::new();
        for c in &self.db.connections {
            if let Some((_, other)) = c.get_pin_first(pin) {
                res.push(other);
            }
        }
        res
    }

    fn eval_pin(&self, pin: Pin, current: &mut HashSet<Pin>, visiting: &mut HashSet<Pin>) -> bool {
        if current.contains(&pin) {
            return true;
        }
        if !visiting.insert(pin) {
            return false;
        }

        for other in self.connected_pins(pin) {
            match self.db.ty(other.ins) {
                InstanceKind::Power => {
                    if self.db.get_power(other.ins).on {
                        current.insert(pin);
                        visiting.remove(&pin);
                        return true;
                    }
                }
                InstanceKind::Wire => {
                    let other_end = Pin {
                        ins: other.ins,
                        index: 1 - other.index,
                    };
                    if self.eval_pin(other_end, current, visiting) {
                        current.insert(pin);
                        visiting.remove(&pin);
                        return true;
                    }
                }
                InstanceKind::Gate(_) => {
                    let out_pin = Pin {
                        ins: other.ins,
                        index: 1,
                    };
                    if other == out_pin {
                        self.eval_instance(other.ins, current);
                        if current.contains(&out_pin) {
                            current.insert(pin);
                            visiting.remove(&pin);
                            return true;
                        }
                    }
                }
            }
        }
        visiting.remove(&pin);
        false
    }
}
