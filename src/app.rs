use std::collections::HashSet;
use std::fmt::Write as _;
use std::hash::Hash;

use egui::{
    Align, Button, Color32, CornerRadius, Image, Layout, Pos2, Rect, Sense, Stroke, StrokeKind, Ui,
    Vec2, Widget as _, pos2, vec2,
};
use slotmap::{Key as _, SecondaryMap, SlotMap};

use crate::{assets, config::CanvasConfig};

const EDGE_THRESHOLD: f32 = 10.0;
const WIRE_HIT_DISTANCE: f32 = 10.0;

// ---- Colors ----
const COLOR_PIN_DETACH_HINT: Color32 = Color32::RED;
const COLOR_PIN_POWERED_OUTLINE: Color32 = Color32::BLUE;
const COLOR_WIRE_POWERED: Color32 = Color32::BLUE;
const COLOR_WIRE_IDLE: Color32 = Color32::DARK_BLUE;
const COLOR_WIRE_HOVER: Color32 = Color32::GREEN;
const COLOR_HOVER_OUTLINE: Color32 = Color32::GRAY;
const COLOR_ENDPOINT_HOVER: Color32 = Color32::LIGHT_YELLOW;
const COLOR_POTENTIAL_CONN_HIGHLIGHT: Color32 = Color32::LIGHT_YELLOW;
const COLOR_SELECTION_HIGHLIGHT: Color32 = Color32::LIGHT_YELLOW;
const COLOR_SELECTION_BOX: Color32 = Color32::YELLOW;

slotmap::new_key_type! {
    pub struct InstanceId;
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub enum InstanceKind {
    Gate(GateKind),
    Power,
    Wire,
}

// A specific pin on an instance
#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone, Eq, PartialEq, Hash)]
struct Pin {
    ins: InstanceId,
    index: u32,
}

// A normalized, order-independent connection between two pins
#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone, Eq)]
struct Connection {
    a: Pin,
    b: Pin,
}

impl Connection {
    fn new(p1: Pin, p2: Pin) -> Self {
        // Normalize by ordering on (ins, index)
        if (p2.ins.data(), p2.index) < (p1.ins.data(), p1.index) {
            Self { a: p2, b: p1 }
        } else {
            Self { a: p1, b: p2 }
        }
    }

    fn involves_instance(&self, id: InstanceId) -> bool {
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

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub enum Drag {
    Panel { pos: Pos2, kind: InstanceKind },
    Canvas { id: InstanceId, offset: Vec2 },
    Resize { id: InstanceId, start: bool },
    Selecting { start: Pos2 },
    MoveSelection { start: Pos2, has_dragged: bool },
}

// Gate

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Gate {
    kind: GateKind,
    // Center position
    pos: Pos2,
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
    fn graphics(&self) -> &assets::InstanceGraphics {
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
    pos: Pos2,
    on: bool,
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
    start: Pos2,
    end: Pos2,
}

impl Wire {}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct DB {
    // Primary key allocator; ensures unique keys across all instance kinds
    instances: SlotMap<InstanceId, ()>,
    // Type registry for each instance id
    types: SecondaryMap<InstanceId, InstanceKind>,
    // Per-kind payloads keyed off the primary key space
    gates: SecondaryMap<InstanceId, Gate>,
    powers: SecondaryMap<InstanceId, Power>,
    wires: SecondaryMap<InstanceId, Wire>,
    connections: HashSet<Connection>,
}

impl DB {
    fn new_gate(&mut self, g: Gate) {
        let k = self.instances.insert(());
        self.gates.insert(k, g);
        let kind = self
            .gates
            .get(k)
            .expect("gate must exist right after insertion")
            .kind;
        self.types.insert(k, InstanceKind::Gate(kind));
    }

    fn new_power(&mut self, p: Power) {
        let k = self.instances.insert(());
        self.powers.insert(k, p);
        self.types.insert(k, InstanceKind::Power);
    }

    fn new_wire(&mut self, w: Wire) {
        let k = self.instances.insert(());
        self.wires.insert(k, w);
        self.types.insert(k, InstanceKind::Wire);
    }

    fn ty(&self, id: InstanceId) -> InstanceKind {
        self.types
            .get(id)
            .copied()
            .expect("instance type missing for id")
    }

    fn get_gate(&self, id: InstanceId) -> &Gate {
        self.gates.get(id).expect("gate not found")
    }

    fn get_gate_mut(&mut self, id: InstanceId) -> &mut Gate {
        self.gates.get_mut(id).expect("gate not found (mut)")
    }

    fn get_power(&self, id: InstanceId) -> &Power {
        self.powers.get(id).expect("power not found")
    }

    fn get_power_mut(&mut self, id: InstanceId) -> &mut Power {
        self.powers.get_mut(id).expect("power not found (mut)")
    }

    fn get_wire(&self, id: InstanceId) -> &Wire {
        self.wires.get(id).expect("wire not found")
    }

    fn get_wire_mut(&mut self, id: InstanceId) -> &mut Wire {
        self.wires.get_mut(id).expect("wire not found (mut)")
    }

    fn pins_of(&self, id: InstanceId) -> Vec<Pin> {
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

    fn pin_position(&self, pin: Pin) -> Pos2 {
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

    fn connected_pins_of_instance(&self, id: InstanceId) -> Vec<Pin> {
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
    canvas_config: CanvasConfig,
    drag: Option<Drag>,
    hovered: Option<InstanceId>,
    db: DB,
    // possible connections while dragging
    potential_connections: HashSet<Connection>,
    // energized pins based on current simulation
    current: HashSet<Pin>,
    // mark when current needs recomputation
    current_dirty: bool,
    show_debug: bool,
    // selection set and move preview
    selected: std::collections::HashSet<InstanceId>,
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

    fn draw_canvas(&mut self, ui: &mut Ui) {
        let (resp, _painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
        let canvas_rect = resp.rect;

        let mouse_up = ui.input(|i| i.pointer.any_released());
        let pointer_pressed = ui.input(|i| i.pointer.primary_down());
        let right_clicked = ui.input(|i| i.pointer.secondary_clicked());
        let mouse_pos = ui.ctx().pointer_interact_pos();

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

    fn draw_gate_preview(&self, ui: &mut Ui, gate_kind: GateKind, pos: Pos2) {
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

    fn draw_power_preview(&self, ui: &mut Ui, pos: Pos2) {
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

    fn draw_wire(&self, ui: &Ui, mouse: Wire, hovered: bool, has_current: bool) {
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

    fn inside_rect(&self, canvas: &Rect, kind: InstanceKind, pos: Pos2) -> bool {
        match kind {
            InstanceKind::Gate(_) | InstanceKind::Power => {
                let rect = Rect::from_center_size(pos, self.canvas_config.base_gate_size);
                canvas.contains_rect(rect)
            }
            InstanceKind::Wire => canvas.contains(pos2(pos.x + 30.0, pos.y)),
        }
    }

    fn interacted_instance(&self, mouse_pos: Pos2) -> Option<InstanceId> {
        // Prioritize smaller/overlay items first: Power over Gates, then Wires
        for (k, power) in &self.db.powers {
            let rect = Rect::from_center_size(power.pos, self.canvas_config.base_gate_size);
            if rect.contains(mouse_pos) {
                return Some(k);
            }
        }

        for (k, gate) in &self.db.gates {
            let rect = Rect::from_center_size(gate.pos, self.canvas_config.base_gate_size);
            if rect.contains(mouse_pos) {
                return Some(k);
            }
        }

        for (k, wire) in &self.db.wires {
            let dist = distance_point_to_segment(mouse_pos, wire.start, wire.end);
            if dist < WIRE_HIT_DISTANCE {
                return Some(k);
            }
        }
        None
    }

    fn handle_drag_start_canvas(&mut self, mouse_pos: Pos2) {
        if self.drag.is_some() {
            return;
        }

        if !self.selected.is_empty() {
            self.drag = Some(Drag::MoveSelection {
                start: mouse_pos,
                has_dragged: false,
            });
            self.potential_connections.clear();
            return;
        }

        if let Some(hovered) = self.hovered {
            match self.db.ty(hovered) {
                InstanceKind::Gate(_) => {
                    if let Some(pin) = self.find_near_pin(hovered, mouse_pos) {
                        self.detach_pin(pin);
                    }
                    let gate = self.db.get_gate(hovered);
                    let offset = gate.pos - mouse_pos;
                    self.drag = Some(Drag::Canvas {
                        id: hovered,
                        offset,
                    });
                }
                InstanceKind::Power => {
                    if let Some(pin) = self.find_near_pin(hovered, mouse_pos) {
                        self.detach_pin(pin);
                    }
                    let power = self.db.get_power(hovered);
                    let offset = power.pos - mouse_pos;
                    self.drag = Some(Drag::Canvas {
                        id: hovered,
                        offset,
                    });
                }
                InstanceKind::Wire => {
                    let wire = self.db.get_wire(hovered);
                    if mouse_pos.distance(wire.start) < EDGE_THRESHOLD {
                        self.detach_pin(Pin {
                            ins: hovered,
                            index: 0,
                        });
                        self.drag = Some(Drag::Resize {
                            id: hovered,
                            start: true,
                        });
                    } else if mouse_pos.distance(wire.end) < EDGE_THRESHOLD {
                        self.detach_pin(Pin {
                            ins: hovered,
                            index: 1,
                        });
                        self.drag = Some(Drag::Resize {
                            id: hovered,
                            start: false,
                        });
                    } else {
                        let wire_center = pos2(
                            (wire.start.x + wire.end.x) * 0.5,
                            (wire.start.y + wire.end.y) * 0.5,
                        );
                        let offset = wire_center - mouse_pos;
                        self.drag = Some(Drag::Canvas {
                            id: hovered,
                            offset,
                        });
                    }
                }
            }
        } else {
            self.drag = Some(Drag::Selecting { start: mouse_pos });
            self.potential_connections.clear();
        }
    }

    fn handle_dragging(&mut self, ui: &mut Ui, mouse: Pos2, canvas_rect: &Rect) {
        match self.drag {
            Some(Drag::Panel { pos: _, kind }) => match kind {
                InstanceKind::Gate(gate_kind) => self.draw_gate_preview(ui, gate_kind, mouse),
                InstanceKind::Power => self.draw_power_preview(ui, mouse),
                InstanceKind::Wire => self.draw_wire(ui, default_wire(mouse), false, false),
            },
            Some(Drag::Selecting { start }) => {
                let min = pos2(start.x.min(mouse.x), start.y.min(mouse.y));
                let max = pos2(start.x.max(mouse.x), start.y.max(mouse.y));
                let rect = Rect::from_min_max(min, max);
                ui.painter().rect_stroke(
                    rect,
                    CornerRadius::default(),
                    Stroke::new(1.5, COLOR_SELECTION_BOX),
                    StrokeKind::Outside,
                );
            }
            Some(Drag::MoveSelection {
                start,
                has_dragged: _,
            }) => {
                let desired = mouse - start;
                if desired != Vec2::ZERO {
                    let group_set = self.collect_connected_instances_from_many(&self.selected);
                    let group: Vec<InstanceId> = group_set.iter().copied().collect();
                    if !group.is_empty() {
                        let delta = self.compute_within_bounds_delta(&group, desired, *canvas_rect);
                        if delta != Vec2::ZERO {
                            self.move_nonwires_and_resize_wires(&group, delta);
                            if let Some(Drag::MoveSelection { start, has_dragged }) =
                                self.drag.as_mut()
                            {
                                *start += delta;
                                *has_dragged = true;
                            }
                        }
                    }
                }
                self.potential_connections.clear();
            }
            Some(Drag::Canvas { id, offset }) => {
                let new_pos = mouse + offset;
                match self.db.ty(id) {
                    InstanceKind::Gate(_) | InstanceKind::Power => {
                        let desired = match self.db.ty(id) {
                            InstanceKind::Gate(_) => {
                                let g = self.db.get_gate(id);
                                new_pos - g.pos
                            }
                            InstanceKind::Power => {
                                let p = self.db.get_power(id);
                                new_pos - p.pos
                            }
                            InstanceKind::Wire => Vec2::ZERO,
                        };
                        let ids = [id];
                        let moved_delta =
                            self.compute_within_bounds_delta(&ids, desired, *canvas_rect);
                        if moved_delta != Vec2::ZERO {
                            self.move_nonwires_and_resize_wires(&ids, moved_delta);
                        }
                    }
                    InstanceKind::Wire => {
                        let w = self.db.get_wire_mut(id);
                        let center = pos2((w.start.x + w.end.x) * 0.5, (w.start.y + w.end.y) * 0.5);
                        let desired = new_pos - center;
                        let delta = clamp_wire_move(w, desired, canvas_rect);
                        w.start += delta;
                        w.end += delta;
                    }
                }

                self.potential_connections = self.compute_potential_connections_for_instance(id);
            }
            Some(Drag::Resize { id, start }) => {
                let mut p = mouse;
                p.x = p.x.clamp(canvas_rect.left(), canvas_rect.right());
                p.y = p.y.clamp(canvas_rect.top(), canvas_rect.bottom());
                let wire = self.db.get_wire_mut(id);
                if start {
                    wire.start = p;
                } else {
                    wire.end = p;
                }

                self.potential_connections = self.compute_potential_connections_for_pin(Pin {
                    ins: id,
                    index: u32::from(!start),
                });
            }
            None => {}
        }

        if let Some(Drag::Panel { pos, kind: _ }) = self.drag.as_mut() {
            *pos = mouse;
        }
    }

    fn handle_drag_end(&mut self, canvas_rect: &Rect, mouse_pos: Option<Pos2>) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        match drag {
            Drag::Panel { pos, kind } => {
                if !self.inside_rect(canvas_rect, kind, pos) {
                    return;
                }
                match kind {
                    InstanceKind::Gate(gate_kind) => self.db.new_gate(Gate {
                        kind: gate_kind,
                        pos,
                    }),
                    InstanceKind::Power => self.db.new_power(Power { pos, on: true }),
                    InstanceKind::Wire => self.db.new_wire(default_wire(pos)),
                };
                self.potential_connections.clear();
                self.current_dirty = true;
            }
            Drag::Selecting { start } => {
                let Some(mouse) = mouse_pos else {
                    return;
                };
                let min = pos2(start.x.min(mouse.x), start.y.min(mouse.y));
                let max = pos2(start.x.max(mouse.x), start.y.max(mouse.y));
                let rect = Rect::from_min_max(min, max);
                let mut sel: HashSet<InstanceId> = HashSet::new();
                for (id, g) in &self.db.gates {
                    let r = Rect::from_center_size(g.pos, self.canvas_config.base_gate_size);
                    if rect.contains_rect(r) {
                        sel.insert(id);
                    }
                }
                for (id, p) in &self.db.powers {
                    let r = Rect::from_center_size(p.pos, self.canvas_config.base_gate_size);
                    if rect.contains_rect(r) {
                        sel.insert(id);
                    }
                }
                for (id, w) in &self.db.wires {
                    if rect.contains(w.start) && rect.contains(w.end) {
                        sel.insert(id);
                    }
                }
                self.selected = sel;
                self.potential_connections.clear();
            }
            Drag::MoveSelection {
                start: _,
                has_dragged,
            } => {
                if !has_dragged {
                    self.selected.clear();
                }
                self.potential_connections.clear();
            }
            Drag::Canvas { id, offset: _ } => {
                if !self.potential_connections.is_empty() {
                    self.finalize_connections_for_instance(id, canvas_rect);
                }
                self.potential_connections.clear();
            }
            Drag::Resize { id, start } => {
                if !self.potential_connections.is_empty() {
                    let pin = Pin {
                        ins: id,
                        index: u32::from(!start),
                    };
                    self.finalize_connections_for_pin(pin, canvas_rect);
                }
                self.potential_connections.clear();
            }
        }
    }

    fn compute_potential_connections_for_instance(&self, id: InstanceId) -> HashSet<Connection> {
        let mut out = HashSet::new();
        for my_pin in self.db.pins_of(id) {
            let pos = self.db.pin_position(my_pin);
            for (other_id, _) in &self.db.types {
                if other_id == id {
                    continue;
                }
                for other_pin in self.db.pins_of(other_id) {
                    let other_pos = self.db.pin_position(other_pin);
                    if (pos - other_pos).length() <= EDGE_THRESHOLD {
                        out.insert(Connection::new(my_pin, other_pin));
                    }
                }
            }
        }
        out
    }

    fn compute_potential_connections_for_pin(&self, pin: Pin) -> HashSet<Connection> {
        let mut out = HashSet::new();
        let pos = self.db.pin_position(pin);
        for (other_id, _) in &self.db.types {
            if other_id == pin.ins {
                continue;
            }
            for other_pin in self.db.pins_of(other_id) {
                let other_pos = self.db.pin_position(other_pin);
                if (pos - other_pos).length() <= EDGE_THRESHOLD {
                    out.insert(Connection::new(pin, other_pin));
                }
            }
        }
        out
    }

    fn finalize_connections_for_instance(&mut self, id: InstanceId, canvas_rect: &Rect) {
        let to_add: Vec<Connection> = self
            .potential_connections
            .iter()
            .copied()
            .filter(|c| c.involves_instance(id))
            .collect();
        for c in &to_add {
            let (moving_pin, other_pin) = if c.a.ins == id {
                (c.a, c.b)
            } else {
                (c.b, c.a)
            };
            self.snap_pin_to_other(moving_pin, other_pin, canvas_rect);
        }

        let mut new_set = HashSet::with_capacity(self.db.connections.len());
        for c in &self.db.connections {
            if c.involves_instance(id) {
                let p1 = self.db.pin_position(c.a);
                let p2 = self.db.pin_position(c.b);
                if (p1 - p2).length() <= EDGE_THRESHOLD {
                    new_set.insert(*c);
                }
            } else {
                new_set.insert(*c);
            }
        }
        for c in to_add {
            new_set.insert(c);
        }
        self.db.connections = new_set;
        self.current_dirty = true;
    }

    fn finalize_connections_for_pin(&mut self, pin: Pin, canvas_rect: &Rect) {
        let to_add: Vec<Connection> = self
            .potential_connections
            .iter()
            .copied()
            .filter(|c| c.a == pin || c.b == pin)
            .collect();
        for c in &to_add {
            if c.a == pin {
                self.snap_pin_to_other(c.a, c.b, canvas_rect);
            }
            if c.b == pin {
                self.snap_pin_to_other(c.b, c.a, canvas_rect);
            }
        }
        // Rebuild connections set, dropping stale ones for this pin
        let mut new_set = HashSet::with_capacity(self.db.connections.len());
        for c in &self.db.connections {
            if c.a == pin || c.b == pin {
                let p1 = self.db.pin_position(c.a);
                let p2 = self.db.pin_position(c.b);
                if (p1 - p2).length() <= EDGE_THRESHOLD {
                    new_set.insert(*c);
                }
            } else {
                new_set.insert(*c);
            }
        }
        for c in to_add {
            new_set.insert(c);
        }
        self.db.connections = new_set;
        self.current_dirty = true;
    }

    fn snap_pin_to_other(&mut self, src: Pin, dst: Pin, canvas_rect: &Rect) {
        let target = self.db.pin_position(dst);
        match self.db.ty(src.ins) {
            InstanceKind::Wire => {
                let w = self.db.get_wire_mut(src.ins);
                if src.index == 0 {
                    w.start = target;
                } else {
                    w.end = target;
                }
            }
            InstanceKind::Gate(gk) => {
                let g = self.db.get_gate_mut(src.ins);
                let info = gk.graphics().pins[src.index as usize];
                let current = g.pos + info.offset;
                let desired = target - current;
                let half = self.canvas_config.base_gate_size * 0.5;
                let delta = clamp_gate_move(g.pos, desired, canvas_rect, half);
                g.pos += delta;
            }
            InstanceKind::Power => {
                let p = self.db.get_power_mut(src.ins);
                let info = assets::POWER_ON_GRAPHICS.pins[src.index as usize];
                let current = p.pos + info.offset;
                let desired = target - current;
                let half = self.canvas_config.base_gate_size * 0.5;
                let delta = clamp_gate_move(p.pos, desired, canvas_rect, half);
                p.pos += delta;
            }
        }
    }

    fn find_near_pin(&self, id: InstanceId, mouse: Pos2) -> Option<Pin> {
        for pin in self.db.pins_of(id) {
            let p = self.db.pin_position(pin);
            if mouse.distance(p) <= EDGE_THRESHOLD {
                return Some(pin);
            }
        }
        None
    }

    fn detach_pin(&mut self, pin: Pin) {
        let mut new_set = HashSet::with_capacity(self.db.connections.len());
        for c in &self.db.connections {
            if c.a == pin || c.b == pin {
                // drop it
            } else {
                new_set.insert(*c);
            }
        }
        self.db.connections = new_set;
        self.current_dirty = true;
    }

    fn collect_connected_instances_from_many(
        &self,
        roots: &HashSet<InstanceId>,
    ) -> HashSet<InstanceId> {
        let mut out: HashSet<InstanceId> = HashSet::new();
        let mut seen: HashSet<InstanceId> = HashSet::new();
        let mut stack: Vec<InstanceId> = roots.iter().copied().collect();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            if !matches!(self.db.ty(id), InstanceKind::Wire) {
                out.insert(id);
            }
            for pin in self.db.connected_pins_of_instance(id) {
                stack.push(pin.ins);
            }
        }
        out
    }

    fn compute_within_bounds_delta(&self, ids: &[InstanceId], desired: Vec2, rect: Rect) -> Vec2 {
        let half_w = self.canvas_config.base_gate_size.x * 0.5;
        let half_h = self.canvas_config.base_gate_size.y * 0.5;
        let mut dx_min = f32::NEG_INFINITY;
        let mut dx_max = f32::INFINITY;
        let mut dy_min = f32::NEG_INFINITY;
        let mut dy_max = f32::INFINITY;

        for id in ids {
            match self.db.ty(*id) {
                InstanceKind::Gate(_) => {
                    let g = self.db.get_gate(*id);
                    let q = g.pos;
                    dx_min = dx_min.max(rect.left() + half_w - q.x);
                    dx_max = dx_max.min(rect.right() - half_w - q.x);
                    dy_min = dy_min.max(rect.top() + half_h - q.y);
                    dy_max = dy_max.min(rect.bottom() - half_h - q.y);
                }
                InstanceKind::Power => {
                    let p = self.db.get_power(*id);
                    let q = p.pos;
                    dx_min = dx_min.max(rect.left() + half_w - q.x);
                    dx_max = dx_max.min(rect.right() - half_w - q.x);
                    dy_min = dy_min.max(rect.top() + half_h - q.y);
                    dy_max = dy_max.min(rect.bottom() - half_h - q.y);
                }
                InstanceKind::Wire => {
                    let w = self.db.get_wire(*id);
                    for q in [w.start, w.end] {
                        dx_min = dx_min.max(rect.left() - q.x);
                        dx_max = dx_max.min(rect.right() - q.x);
                        dy_min = dy_min.max(rect.top() - q.y);
                        dy_max = dy_max.min(rect.bottom() - q.y);
                    }
                }
            }
        }
        let safe_dx = desired.x.clamp(dx_min, dx_max);
        let safe_dy = desired.y.clamp(dy_min, dy_max);
        vec2(safe_dx, safe_dy)
    }

    fn move_nonwires_and_resize_wires(&mut self, ids: &[InstanceId], delta: Vec2) {
        // Move all non-wire instances, then adjust connected wire endpoints
        for id in ids {
            match self.db.ty(*id) {
                InstanceKind::Gate(_) => {
                    let g = self.db.get_gate_mut(*id);
                    g.pos += delta;
                }
                InstanceKind::Power => {
                    let p = self.db.get_power_mut(*id);
                    p.pos += delta;
                }
                InstanceKind::Wire => {}
            }
        }

        // Resize wire endpoints attached to any moved instance
        for id in ids {
            for pin in self.db.connected_pins_of_instance(*id) {
                if matches!(self.db.ty(pin.ins), InstanceKind::Wire) {
                    let w = self.db.get_wire_mut(pin.ins);
                    if pin.index == 0 {
                        w.start += delta;
                    } else {
                        w.end += delta;
                    }
                }
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

fn default_wire(pos: Pos2) -> Wire {
    Wire {
        start: pos2(pos.x - 30.0, pos.y),
        end: pos2(pos.x + 30.0, pos.y),
    }
}

fn distance_point_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
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

fn clamp_gate_move(current: Pos2, desired: Vec2, rect: &Rect, half: Vec2) -> Vec2 {
    let target = current + desired;
    let clamped_x = target.x.clamp(rect.left() + half.x, rect.right() - half.x);
    let clamped_y = target.y.clamp(rect.top() + half.y, rect.bottom() - half.y);
    vec2(clamped_x - current.x, clamped_y - current.y)
}

fn clamp_wire_move(wire: &Wire, desired: Vec2, rect: &Rect) -> Vec2 {
    let ts = wire.start + desired;
    let te = wire.end + desired;
    let sx = ts.x.clamp(rect.left(), rect.right());
    let sy = ts.y.clamp(rect.top(), rect.bottom());
    let ex = te.x.clamp(rect.left(), rect.right());
    let ey = te.y.clamp(rect.top(), rect.bottom());
    let safe_dx_start = sx - wire.start.x;
    let safe_dy_start = sy - wire.start.y;
    let safe_dx_end = ex - wire.end.x;
    let safe_dy_end = ey - wire.end.y;
    let safe_dx = if desired.x.is_sign_positive() {
        safe_dx_start.min(safe_dx_end)
    } else {
        safe_dx_start.max(safe_dx_end)
    };
    let safe_dy = if desired.y.is_sign_positive() {
        safe_dy_start.min(safe_dy_end)
    } else {
        safe_dy_start.max(safe_dy_end)
    };
    vec2(safe_dx, safe_dy)
}
