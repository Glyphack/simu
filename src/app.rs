use std::collections::HashSet;
use std::fmt::{Display, Write as _};
use std::hash::Hash;

use egui::{
    Align, Button, Color32, CornerRadius, Image, Layout, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2, Widget as _, pos2, vec2,
};
use slotmap::{SecondaryMap, SlotMap};

use crate::simulator::{SimulationStatus, Simulator, Value};
use crate::{
    assets::{self},
    config::CanvasConfig,
    connection_manager::{Connection, ConnectionManager},
    custom_circuit::{self, Module, ModuleDefinition},
    drag::Drag,
};

pub const PANEL_BUTTON_MAX_HEIGHT: f32 = 50.0;

pub const LABEL_EDIT_TEXT_SIZE: f32 = 16.0;
pub const LABEL_DISPLAY_TEXT_SIZE: f32 = 19.0;

// Grid
pub const GRID_SIZE: f32 = 20.0;
pub const COLOR_GRID_LIGHT: Color32 = Color32::from_rgb(230, 230, 230);
pub const COLOR_GRID_DARK: Color32 = Color32::from_rgb(40, 40, 40);

pub const COLOR_PIN_DETACH_HINT: Color32 = Color32::RED;
pub const COLOR_PIN_POWERED_OUTLINE: Color32 = Color32::GREEN;
pub const COLOR_WIRE_POWERED: Color32 = Color32::GREEN;
pub const COLOR_WIRE_IDLE: Color32 = Color32::LIGHT_BLUE;
// Hover
pub const COLOR_WIRE_HOVER: Color32 = Color32::GRAY;
pub const COLOR_HOVER_INSTANCE_OUTLINE: Color32 = Color32::GRAY;
pub const COLOR_HOVER_PIN_TO_WIRE: Color32 = Color32::GRAY;
pub const COLOR_HOVER_PIN_DETACH: Color32 = Color32::RED;
pub const PIN_HOVER_THRESHOLD: f32 = 8.0;

pub const INSTANEC_OUTLINE: Vec2 = vec2(6.0, 6.0);
pub const INSTANEC_OUTLINE_TICKNESS: f32 = 2.0;

pub const NEW_PIN_ON_WIRE_THRESHOLD: f32 = 10.0;

// Connections
pub const COLOR_POTENTIAL_CONN_HIGHLIGHT: Color32 = Color32::LIGHT_BLUE;
pub const WIRE_HIT_DISTANCE: f32 = 10.0;
pub const SNAP_THRESHOLD: f32 = 10.0;
pub const PIN_MOVE_HINT_D: f32 = 10.0;
pub const PIN_MOVE_HINT_COLOR: Color32 = Color32::GRAY;

pub const COLOR_SELECTION_HIGHLIGHT: Color32 = Color32::GRAY;
pub const COLOR_SELECTION_BOX: Color32 = Color32::LIGHT_BLUE;

pub const MIN_WIRE_SIZE: f32 = 40.0;

slotmap::new_key_type! {
    pub struct InstanceId;
}

impl Display for InstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("{:?}", self.0))
    }
}

impl From<u32> for InstanceId {
    fn from(value: u32) -> Self {
        Self(slotmap::KeyData::from_ffi(value as u64))
    }
}

slotmap::new_key_type! {
    pub struct LabelId;
}

#[derive(serde::Deserialize, serde::Serialize, Eq, PartialEq, Hash, Copy, Debug, Clone)]
pub enum Hover {
    Pin(Pin),
    Instance(InstanceId),
}

impl Hover {
    pub fn instance(&self) -> InstanceId {
        match self {
            Self::Pin(pin) => pin.ins,
            Self::Instance(instance_id) => *instance_id,
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub enum ClipBoardItem {
    Gate(GateKind, Vec2),
    Power(Vec2),
    Wire(Vec2, Vec2),
    Lamp(Vec2),
    Clock(Vec2),
    // Index to definition
    CustomCircuit(usize, Vec2),
    Label(String, Vec2),
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub enum InstanceKind {
    Gate(GateKind),
    Power,
    Wire,
    Lamp,
    Clock,
    CustomCircuit(usize),
}

// A specific pin on an instance
#[derive(
    serde::Deserialize, serde::Serialize, Copy, Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd,
)]
pub struct Pin {
    pub ins: InstanceId,
    pub index: u32,
}

impl Pin {
    pub fn display(&self, db: &DB) -> String {
        let instance_display = match db.ty(self.ins) {
            InstanceKind::Gate(_) => {
                let gate = db.get_gate(self.ins);
                gate.display(db)
            }
            InstanceKind::Power => {
                let power = db.get_power(self.ins);
                power.display(db)
            }
            InstanceKind::Wire => {
                let wire = db.get_wire(self.ins);
                wire.display(db)
            }
            InstanceKind::Lamp => {
                let lamp = db.get_lamp(self.ins);
                lamp.display(db)
            }
            InstanceKind::Clock => {
                let clock = db.get_clock(self.ins);
                clock.display(db)
            }
            InstanceKind::CustomCircuit(_) => format!("CustomCircuit {{ id: {:?} }}", self.ins),
        };
        let pin_info = db.pin_info(*self);
        format!(
            "{:?} pin#{} in {} ",
            pin_info.kind, self.index, instance_display,
        )
    }
}

/// Information about a pin's direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinInfo {
    pub kind: crate::assets::PinKind,
}

// Gate

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Gate {
    // Center position
    pub pos: Pos2,
    pub kind: GateKind,
}

#[derive(serde::Deserialize, serde::Serialize, PartialEq, Eq, Copy, Debug, Clone)]
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

impl Gate {
    pub fn display(&self, db: &DB) -> String {
        // Find the InstanceId for this gate in the database
        for (id, gate) in &db.gates {
            if gate.pos == self.pos && gate.kind == self.kind {
                return format!("({:?} ({}))", self.kind, id);
            }
        }
        format!(
            "Gate {{ kind: {:?}, pos: ({:.1}, {:.1}) }} - not found in DB",
            self.kind, self.pos.x, self.pos.y
        )
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
    pub fn display(&self, db: &DB) -> String {
        // Find the InstanceId for this power in the database
        for (id, power) in &db.powers {
            if power.pos == self.pos && power.on == self.on {
                return format!("Power {{ id: {}, on: {}}}", id, self.on);
            }
        }
        format!(
            "Power {{ on: {}, pos: ({:.1}, {:.1}) }} - not found in DB",
            self.on, self.pos.x, self.pos.y
        )
    }

    fn graphics(&self) -> &assets::InstanceGraphics {
        if self.on {
            &assets::POWER_ON_GRAPHICS
        } else {
            &assets::POWER_OFF_GRAPHICS
        }
    }
}

// Power end

// Lamp

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Lamp {
    pub pos: Pos2,
}

impl Lamp {
    pub fn display(&self, db: &DB) -> String {
        // Find the InstanceId for this lamp in the database
        for (id, lamp) in &db.lamps {
            if lamp.pos == self.pos {
                return format!("Lamp {{ id: {id}}}");
            }
        }
        format!(
            "Lamp {{ pos: ({:.1}, {:.1}) }} - not found in DB",
            self.pos.x, self.pos.y
        )
    }

    fn graphics(&self) -> &assets::InstanceGraphics {
        &assets::LAMP_GRAPHICS
    }
}

// Lamp end

// Clock

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Clock {
    pub pos: Pos2,
    pub period: u32, // Placeholder for future use
}

impl Clock {
    pub fn display(&self, db: &DB) -> String {
        for (id, clock) in &db.clocks {
            if clock.pos == self.pos {
                return format!("Clock {{ id: {id}}}");
            }
        }
        format!(
            "Clock {{ pos: ({:.1}, {:.1}) }} - not found in DB",
            self.pos.x, self.pos.y
        )
    }

    fn graphics(&self) -> &assets::InstanceGraphics {
        &assets::CLOCK_GRAPHICS
    }
}

// Clock end

// Label

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Label {
    pub pos: Pos2,
    pub text: String,
}

/// Label is a visual annotation that user can place anywhere in the canvas.
/// Therefore it is not an instance like Gate because it does nothing.
/// It is handled separately in the code.
impl Label {
    pub fn new(pos: Pos2) -> Self {
        Self {
            pos,
            text: String::from("Label"),
        }
    }
}

// Label end

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Wire {
    pub start: Pos2,
    pub end: Pos2,
}

impl Wire {
    pub fn display(&self, db: &DB) -> String {
        // Find the InstanceId for this wire in the database
        for (id, wire) in &db.wires {
            if wire.start == self.start && wire.end == self.end {
                return format!("Wire {id}");
            }
        }
        format!(
            "Wire {{ start: ({:.1}, {:.1}), end: ({:.1}, {:.1}) }} - not found in DB",
            self.start.x, self.start.y, self.end.x, self.end.y
        )
    }

    pub fn new_at(pos: Pos2) -> Self {
        Self::new(pos2(pos.x - 30.0, pos.y), pos2(pos.x + 30.0, pos.y))
    }
    pub fn new(start: Pos2, end: Pos2) -> Self {
        Self { start, end }
    }

    pub fn closest_point_on_line(&self, p: Pos2) -> Pos2 {
        let a = self.start;
        let b = self.end;
        let ab: Vec2 = b - a;
        let ap: Vec2 = p - a;

        let ab_len2 = ab.x * ab.x + ab.y * ab.y;
        if ab_len2 == 0.0 {
            return a;
        }

        let t = ((ap.x * ab.x + ap.y * ab.y) / ab_len2).clamp(0.0, 1.0);

        a + ab * t
    }

    pub fn dist_to_closest_point_on_line(&self, p: Pos2) -> f32 {
        let closest = self.closest_point_on_line(p);
        (p - closest).length()
    }

    pub fn center(&self) -> Pos2 {
        pos2(
            (self.start.x + self.end.x) * 0.5,
            (self.start.y + self.end.y) * 0.5,
        )
    }
}

#[derive(Default, serde::Deserialize, serde::Serialize, Clone)]
pub struct DB {
    // Primary key allocator; ensures unique keys across all instance kinds
    pub instances: SlotMap<InstanceId, ()>,
    // Type registry for each instance id
    pub types: SecondaryMap<InstanceId, InstanceKind>,
    // Per-kind payloads keyed off the primary key space
    pub gates: SecondaryMap<InstanceId, Gate>,
    pub powers: SecondaryMap<InstanceId, Power>,
    pub wires: SecondaryMap<InstanceId, Wire>,
    pub lamps: SecondaryMap<InstanceId, Lamp>,
    pub clocks: SecondaryMap<InstanceId, Clock>,
    pub modules: SecondaryMap<InstanceId, Module>,
    // Definition of custom circuits created by the user
    pub module_definitions: Vec<ModuleDefinition>,
    pub connections: HashSet<Connection>,
    // Labels
    pub labels: SlotMap<LabelId, Label>,
}

impl DB {
    pub fn new() -> Self {
        Self::default()
    }
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

    pub fn new_lamp(&mut self, l: Lamp) -> InstanceId {
        let k = self.instances.insert(());
        self.lamps.insert(k, l);
        self.types.insert(k, InstanceKind::Lamp);
        k
    }

    pub fn new_clock(&mut self, c: Clock) -> InstanceId {
        let k = self.instances.insert(());
        self.clocks.insert(k, c);
        self.types.insert(k, InstanceKind::Clock);
        k
    }

    pub fn new_custom_circuit(&mut self, c: crate::custom_circuit::Module) -> InstanceId {
        let k = self.instances.insert(());
        let definition_index = c.definition_index;
        self.modules.insert(k, c);
        self.types
            .insert(k, InstanceKind::CustomCircuit(definition_index));
        k
    }

    pub fn ty(&self, id: InstanceId) -> InstanceKind {
        self.types
            .get(id)
            .copied()
            .unwrap_or_else(|| panic!("instance type missing for id: {id:?}"))
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

    pub fn get_lamp(&self, id: InstanceId) -> &Lamp {
        self.lamps.get(id).expect("lamp not found")
    }

    pub fn get_lamp_mut(&mut self, id: InstanceId) -> &mut Lamp {
        self.lamps.get_mut(id).expect("lamp not found (mut)")
    }

    pub fn get_clock(&self, id: InstanceId) -> &Clock {
        self.clocks.get(id).expect("clock not found")
    }

    pub fn get_clock_mut(&mut self, id: InstanceId) -> &mut Clock {
        self.clocks.get_mut(id).expect("clock not found (mut)")
    }

    pub fn get_custom_circuit(&self, id: InstanceId) -> &crate::custom_circuit::Module {
        self.modules.get(id).expect("custom circuit not found")
    }

    pub fn get_custom_circuit_mut(&mut self, id: InstanceId) -> &mut crate::custom_circuit::Module {
        self.modules
            .get_mut(id)
            .expect("custom circuit not found (mut)")
    }

    // Pin helper methods with type checking

    // Gates - generic versions
    pub fn gate_inp_n(&self, id: InstanceId, n: u32) -> Pin {
        self.get_gate(id); // Type check
        assert!(n < 2, "Gates only have 2 inputs (0 and 1)");
        Pin {
            ins: id,
            index: if n == 0 { 0 } else { 2 },
        }
    }

    pub fn gate_output_n(&self, id: InstanceId, n: u32) -> Pin {
        self.get_gate(id); // Type check
        assert!(n == 0, "Gates only have 1 output");
        Pin { ins: id, index: 1 }
    }

    pub fn gate_inp1(&self, id: InstanceId) -> Pin {
        self.gate_inp_n(id, 0)
    }

    pub fn gate_inp2(&self, id: InstanceId) -> Pin {
        self.gate_inp_n(id, 1)
    }

    pub fn gate_output(&self, id: InstanceId) -> Pin {
        self.gate_output_n(id, 0)
    }

    pub fn wire_pin_n(&self, id: InstanceId, n: u32) -> Pin {
        self.get_wire(id); // Type check
        assert!(n < 2, "Wires only have 2 pins (0 and 1)");
        Pin { ins: id, index: n }
    }

    pub fn wire_start(&self, id: InstanceId) -> Pin {
        self.wire_pin_n(id, 0)
    }

    pub fn wire_end(&self, id: InstanceId) -> Pin {
        self.wire_pin_n(id, 1)
    }

    pub fn power_output(&self, id: InstanceId) -> Pin {
        self.get_power(id);
        Pin { ins: id, index: 0 }
    }

    pub fn lamp_input(&self, id: InstanceId) -> Pin {
        self.get_lamp(id);
        Pin { ins: id, index: 0 }
    }

    pub fn clock_output(&self, id: InstanceId) -> Pin {
        self.get_clock(id);
        Pin { ins: id, index: 0 }
    }

    // Custom circuits (variable pins)
    pub fn custom_circuit_pin(&self, id: InstanceId, n: u32) -> Pin {
        let cc = self.get_custom_circuit(id);
        let def = &self.module_definitions[cc.definition_index];
        assert!(
            (n as usize) < def.external_pins.len(),
            "Pin index out of bounds for custom circuit"
        );
        Pin { ins: id, index: n }
    }

    /// Get the base pin kind without considering wire connections (avoids recursion)
    fn pin_kind_base(&self, pin: Pin) -> assets::PinKind {
        match self.ty(pin.ins) {
            InstanceKind::Gate(gk) => {
                let graphics = gk.graphics();
                graphics.pins[pin.index as usize].kind
            }
            InstanceKind::Power => {
                let graphics = &assets::POWER_ON_GRAPHICS;
                graphics.pins[pin.index as usize].kind
            }
            InstanceKind::Wire => {
                // For wires, return Input by default to avoid recursion
                assets::PinKind::Input
            }
            InstanceKind::Lamp => {
                let graphics = &assets::LAMP_GRAPHICS;
                graphics.pins[pin.index as usize].kind
            }
            InstanceKind::Clock => {
                let graphics = &assets::CLOCK_GRAPHICS;
                graphics.pins[pin.index as usize].kind
            }
            InstanceKind::CustomCircuit(_) => {
                let cc = self.get_custom_circuit(pin.ins);
                let def = &self.module_definitions[cc.definition_index];
                def.external_pins[pin.index as usize].kind
            }
        }
    }

    /// Get information about a pin's direction (input or output)
    pub fn pin_info(&self, pin: Pin) -> PinInfo {
        let kind = match self.ty(pin.ins) {
            InstanceKind::Wire => {
                // Determine if this wire pin is input or output based on connections
                // The head of wire that is connected to another output is the input pin of the wire.
                // When both heads are not connected start is the input.
                let start = self.wire_start(pin.ins);
                let end = self.wire_end(pin.ins);

                let start_conns = self.connected_pins(start);
                let mut start_connected_to_output = false;
                for conn in start_conns {
                    if self.pin_kind_base(conn) == assets::PinKind::Output {
                        start_connected_to_output = true;
                        break;
                    }
                }

                let end_conns = self.connected_pins(end);
                let mut end_connected_to_output = false;
                for conn in end_conns {
                    if self.pin_kind_base(conn) == assets::PinKind::Output {
                        end_connected_to_output = true;
                        break;
                    }
                }

                // Determine kind based on which pin this is
                if pin.index == 0 {
                    // This is the start pin
                    if start_connected_to_output {
                        assets::PinKind::Input
                    } else if end_connected_to_output {
                        assets::PinKind::Output
                    } else {
                        // Default: start is input
                        assets::PinKind::Input
                    }
                } else {
                    // This is the end pin
                    if end_connected_to_output {
                        assets::PinKind::Input
                    } else if start_connected_to_output {
                        assets::PinKind::Output
                    } else {
                        // Default: end is output (since start is input)
                        assets::PinKind::Output
                    }
                }
            }
            _ => self.pin_kind_base(pin),
        };
        PinInfo { kind }
    }

    pub fn new_label(&mut self, label: Label) -> LabelId {
        self.labels.insert(label)
    }

    pub fn get_label(&self, id: LabelId) -> &Label {
        self.labels.get(id).expect("label not found")
    }

    pub fn get_label_mut(&mut self, id: LabelId) -> &mut Label {
        self.labels.get_mut(id).expect("label not found (mut)")
    }

    pub fn pins_of(&self, id: InstanceId) -> Vec<Pin> {
        match self.ty(id) {
            InstanceKind::Gate(gk) => {
                let n = gk.graphics().pins.len();
                (0..n as u32).map(|i| Pin { ins: id, index: i }).collect()
            }
            InstanceKind::Power => {
                let n = assets::POWER_ON_GRAPHICS.pins.len();
                (0..n as u32).map(|i| Pin { ins: id, index: i }).collect()
            }
            InstanceKind::Wire => vec![Pin { ins: id, index: 0 }, Pin { ins: id, index: 1 }],
            InstanceKind::Lamp => {
                let n = assets::LAMP_GRAPHICS.pins.len();
                (0..n as u32).map(|i| Pin { ins: id, index: i }).collect()
            }
            InstanceKind::Clock => {
                let n = assets::CLOCK_GRAPHICS.pins.len();
                (0..n as u32).map(|i| Pin { ins: id, index: i }).collect()
            }
            InstanceKind::CustomCircuit(_) => {
                let cc = self.get_custom_circuit(id);
                if cc.definition_index < self.module_definitions.len() {
                    let def = &self.module_definitions[cc.definition_index];
                    (0..def.external_pins.len() as u32)
                        .map(|i| Pin { ins: id, index: i })
                        .collect()
                } else {
                    Vec::new()
                }
            }
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
            InstanceKind::Lamp => {
                let l = self.get_lamp(pin.ins);
                let info = l.graphics().pins[pin.index as usize];
                l.pos + info.offset
            }
            InstanceKind::Clock => {
                let c = self.get_clock(pin.ins);
                let info = c.graphics().pins[pin.index as usize];
                c.pos + info.offset
            }
            InstanceKind::CustomCircuit(_) => {
                let cc = self.get_custom_circuit(pin.ins);
                cc.pos + self.pin_offset(pin)
            }
        }
    }

    pub fn pin_offset(&self, pin: Pin) -> Vec2 {
        match self.ty(pin.ins) {
            InstanceKind::Gate(gk) => {
                let info = gk.graphics().pins[pin.index as usize];
                info.offset
            }
            InstanceKind::Power => {
                let p = self.get_power(pin.ins);
                let info = p.graphics().pins[pin.index as usize];
                info.offset
            }
            InstanceKind::Wire => {
                let w = self.get_wire(pin.ins);
                let center = w.center();
                if pin.index == 0 {
                    center - w.start
                } else {
                    center - w.end
                }
            }
            InstanceKind::Lamp => {
                let l = self.get_lamp(pin.ins);
                let info = l.graphics().pins[pin.index as usize];
                info.offset
            }
            InstanceKind::Clock => {
                let c = self.get_clock(pin.ins);
                let info = c.graphics().pins[pin.index as usize];
                info.offset
            }
            InstanceKind::CustomCircuit(_) => {
                let cc = self.get_custom_circuit(pin.ins);
                let def = &self.module_definitions[cc.definition_index];
                def.external_pins[pin.index as usize].offset
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

    // Connected pins to this pin
    pub fn connected_pins(&self, pin: Pin) -> Vec<Pin> {
        let mut res = Vec::new();
        for c in &self.connections {
            if let Some((_, other)) = c.get_pin_first(pin) {
                res.push(other);
            }
        }
        res
    }

    pub fn connected_insntances(&self, id: InstanceId) -> Vec<InstanceId> {
        let mut out = vec![id];
        for c in &self.connections {
            if c.a.ins == id {
                out.push(c.b.ins);
            } else if c.b.ins == id {
                out.push(c.a.ins);
            }
        }
        out
    }

    pub fn move_nonwires_and_resize_wires(&mut self, ids: &[InstanceId], delta: Vec2) {
        let ids_set: HashSet<InstanceId> = ids.iter().copied().collect();

        for id in ids {
            match self.ty(*id) {
                InstanceKind::Gate(_) => {
                    let g = self.get_gate_mut(*id);
                    g.pos += delta;
                }
                InstanceKind::Power => {
                    let p = self.get_power_mut(*id);
                    p.pos += delta;
                }
                InstanceKind::Wire => {
                    let w = self.get_wire_mut(*id);
                    w.start += delta;
                    w.end += delta;
                }
                InstanceKind::Lamp => {
                    let l = self.get_lamp_mut(*id);
                    l.pos += delta;
                }
                InstanceKind::Clock => {
                    let c = self.get_clock_mut(*id);
                    c.pos += delta;
                }
                InstanceKind::CustomCircuit(_) => {
                    let cc = self.get_custom_circuit_mut(*id);
                    cc.pos += delta;
                }
            }
        }

        for id in ids {
            for pin in self.connected_pins_of_instance(*id) {
                if matches!(self.ty(pin.ins), InstanceKind::Wire) && !ids_set.contains(&pin.ins) {
                    // Otherwise resize the wire
                    let w = self.get_wire_mut(pin.ins);
                    if pin.index == 0 {
                        w.start += delta;
                    } else {
                        w.end += delta;
                    }
                }
            }
        }
    }

    pub fn move_instance_and_propagate(&mut self, id: InstanceId, delta: Vec2) {
        let mut visited = HashSet::new();
        self.move_instance_and_propagate_recursive(id, delta, &mut visited);
    }

    fn move_instance_and_propagate_recursive(
        &mut self,
        id: InstanceId,
        delta: Vec2,
        visited: &mut HashSet<InstanceId>,
    ) {
        if !visited.insert(id) {
            return;
        }

        // Move this instance
        match self.ty(id) {
            InstanceKind::Gate(_) => {
                let g = self.get_gate_mut(id);
                g.pos += delta;
            }
            InstanceKind::Power => {
                let p = self.get_power_mut(id);
                p.pos += delta;
            }
            InstanceKind::Wire => {
                let w = self.get_wire_mut(id);
                w.start += delta;
                w.end += delta;
            }
            InstanceKind::Lamp => {
                let l = self.get_lamp_mut(id);
                l.pos += delta;
            }
            InstanceKind::Clock => {
                let c = self.get_clock_mut(id);
                c.pos += delta;
            }
            InstanceKind::CustomCircuit(_) => {
                let cc = self.get_custom_circuit_mut(id);
                cc.pos += delta;
            }
        }

        // Get connected instances before we recurse
        let connected = self.connected_insntances(id);

        // Process each connected instance
        for connected_id in connected {
            if connected_id == id || visited.contains(&connected_id) {
                continue;
            }

            match self.ty(connected_id) {
                InstanceKind::Wire => {
                    // For wires, resize them to stay connected
                    // Find which pin of the wire is connected to our moved instance
                    let wire_pins = self.pins_of(connected_id);
                    for wire_pin in wire_pins {
                        // Check if this wire pin is connected to any pin of our moved instance
                        for moved_pin in self.pins_of(id) {
                            if self
                                .connections
                                .contains(&Connection::new(wire_pin, moved_pin))
                            {
                                // Update the wire endpoint to match the new pin position
                                let new_pin_pos = self.pin_position(moved_pin);
                                let w = self.get_wire_mut(connected_id);
                                if wire_pin.index == 0 {
                                    w.start = new_pin_pos;
                                } else {
                                    w.end = new_pin_pos;
                                }
                            }
                        }
                    }
                    // Mark as visited but don't propagate further (wires are endpoints)
                    visited.insert(connected_id);
                }
                InstanceKind::Gate(_)
                | InstanceKind::Power
                | InstanceKind::Lamp
                | InstanceKind::Clock
                | InstanceKind::CustomCircuit(_) => {
                    // For non-wires, propagate the same delta
                    self.move_instance_and_propagate_recursive(connected_id, delta, visited);
                }
            }
        }
    }
}

pub fn current_dirty() -> bool {
    true
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockState {
    Stopped,
    Running,
}

#[derive(Debug, Clone)]
pub struct ClockController {
    pub voltage: bool,
    pub state: ClockState,
    pub tick_accumulator: f32,
    pub tick_interval: f32, // seconds between ticks
}

impl Default for ClockController {
    fn default() -> Self {
        Self {
            voltage: false,
            state: ClockState::Running,
            tick_accumulator: 0.0,
            tick_interval: 0.5, // 0.5 seconds = 2 Hz
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
pub struct App {
    pub canvas_config: CanvasConfig,
    pub drag: Option<Drag>,
    pub hovered: Option<Hover>,
    pub db: DB,
    // connection manager for handling spatial indexing and validation
    #[serde(skip)]
    pub connection_manager: ConnectionManager,
    // possible connections while dragging
    pub potential_connections: HashSet<Connection>,
    // mark when current needs recomputation
    #[serde(skip, default = "current_dirty")]
    pub current_dirty: bool,
    pub show_debug: bool,
    // selection set and move preview
    // TODO: Selection is not handling labels.
    pub selected: HashSet<InstanceId>,
    pub drag_had_movement: bool,
    //Copied. Items with their offset compared to a middle point in the rectangle
    pub clipboard: Vec<ClipBoardItem>,
    // Where are we in the world
    pub viewport_offset: Vec2,
    // For web load functionality - stores pending JSON to load
    #[serde(skip)]
    pub pending_load_json: Option<String>,
    #[serde(skip)]
    pub panning: bool,
    #[serde(skip)]
    pub panel_width: f32,
    // Label editing state
    #[serde(skip)]
    pub editing_label: Option<LabelId>,
    #[serde(skip)]
    pub label_edit_buffer: String,
    // Simulation service - holds simulation state and results
    #[serde(skip)]
    pub simulator: Simulator,
    // Clock controller for managing clock ticking
    #[serde(skip, default = "ClockController::default")]
    pub clock_controller: ClockController,
}

impl Default for App {
    fn default() -> Self {
        let db = DB::default();
        let c = ConnectionManager::new(&db);
        Self {
            db,
            canvas_config: Default::default(),
            drag: Default::default(),
            hovered: Default::default(),
            connection_manager: c,
            potential_connections: Default::default(),
            current_dirty: true,
            show_debug: true,
            selected: Default::default(),
            drag_had_movement: false,
            clipboard: Default::default(),
            pending_load_json: None,
            viewport_offset: Vec2::ZERO,
            panning: false,
            panel_width: 0.0,
            editing_label: None,
            label_edit_buffer: String::new(),
            simulator: Simulator::default(),
            clock_controller: ClockController::default(),
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

                ui.menu_button("File", |ui| {
                    if ui.button("Save Circuit").clicked()
                        && let Err(e) = self.save_to_file()
                    {
                        log::error!("Failed to save circuit: {e}");
                    }
                    if ui.button("Load Circuit").clicked()
                        && let Err(e) = self.load_from_file()
                    {
                        log::error!("Failed to load circuit: {e}");
                    }
                    if !is_web {
                        ui.separator();
                        if ui.button("Quit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                });
                ui.add_space(16.0);

                ui.menu_button("View", |ui| {
                    ui.checkbox(&mut self.show_debug, "World Debug");
                });
                ui.add_space(16.0);

                ui.menu_button("Tools", |ui| {
                    if ui.button("Create module").clicked() {
                        self.create_module();
                    }
                });
                ui.add_space(16.0);

                // Clock controls
                ui.label("Clock:");
                if ui.button("⏹ Stop").clicked() {
                    self.clock_controller.state = ClockState::Stopped;
                }
                if ui.button("⏭ Step").clicked() {
                    self.clock_controller.voltage = !self.clock_controller.voltage;
                    self.current_dirty = true;
                }
                if ui.button("▶ Start").clicked() {
                    self.clock_controller.state = ClockState::Running;
                    self.clock_controller.tick_accumulator = 0.0;
                }
                ui.add_space(8.0);

                // Clock speed slider
                ui.label("Speed:");
                let mut speed_hz = 1.0 / self.clock_controller.tick_interval;
                if ui
                    .add(egui::Slider::new(&mut speed_hz, 0.5..=5.0).text("Hz"))
                    .changed()
                {
                    self.clock_controller.tick_interval = 1.0 / speed_hz;
                }
                ui.add_space(16.0);

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    egui::widgets::global_theme_preference_buttons(ui);

                    ui.add_space(16.0);
                });
            });
        });

        let dt = ctx.input(|i| i.stable_dt);
        let should_tick = match self.clock_controller.state {
            ClockState::Running => {
                self.clock_controller.tick_accumulator += dt;
                if self.clock_controller.tick_accumulator >= self.clock_controller.tick_interval {
                    self.clock_controller.tick_accumulator -= self.clock_controller.tick_interval;
                    true
                } else {
                    false
                }
            }
            ClockState::Stopped => false,
        };

        if should_tick {
            self.clock_controller.voltage = !self.clock_controller.voltage;
            self.simulator.clocks_on = self.clock_controller.voltage;
            self.current_dirty = true;
        }

        // Request continuous repaint if clock is running
        if self.clock_controller.state == ClockState::Running {
            ctx.request_repaint();
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            self.draw_main(ui);
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

    fn is_on(&self, pin: Pin) -> bool {
        let Some(v) = self.simulator.current.get(&pin) else {
            return false;
        };

        *v == Value::One
    }

    pub fn screen_to_world(&self, pos: Pos2) -> Pos2 {
        pos + self.viewport_offset
    }

    pub fn draw_main(&mut self, ui: &mut Ui) {
        self.process_pending_load();

        if self.show_debug {
            egui::Window::new("Debug logs").show(ui.ctx(), |ui| {
                egui_logger::logger_ui().show(ui);
            });
        }
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            self.canvas_config = CanvasConfig::default();
            if self.show_debug {
                let full_h = ui.available_height();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut dbg = self.debug_string(ui);
                    ui.add_sized(vec2(320.0, full_h), egui::TextEdit::multiline(&mut dbg));
                });
            }

            let panel_rect = ui
                .vertical(|ui| {
                    ui.heading("Logic Gates");
                    self.draw_panel(ui);
                })
                .response
                .rect;
            self.panel_width = panel_rect.width();
            ui.separator();
            ui.vertical(|ui| {
                ui.heading("Canvas");
                ui.label("press backspace/d to remove object");
                ui.label("right click on powers to toggle");
                ui.label("right click on canvas to drag");
                self.draw_canvas(ui);
            });
        });
    }

    fn draw_panel(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([true, false])
            .show(ui, |ui| {
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::And));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Nand));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Or));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Nor));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Xor));
                self.draw_panel_button(ui, InstanceKind::Gate(GateKind::Xnor));
                self.draw_panel_button(ui, InstanceKind::Power);
                self.draw_panel_button(ui, InstanceKind::Lamp);
                self.draw_panel_button(ui, InstanceKind::Clock);
                self.draw_panel_button(ui, InstanceKind::Wire);

                ui.add_space(8.0);
                self.draw_label_button(ui);

                if !self.db.module_definitions.is_empty() {
                    ui.add_space(8.0);
                    ui.label("Custom Circuits:");
                }
                let custom_circuit_indices: Vec<usize> =
                    (0..self.db.module_definitions.len()).collect();
                for i in custom_circuit_indices {
                    self.draw_panel_button(ui, InstanceKind::CustomCircuit(i));
                }

                ui.add_space(8.0);

                if Button::new("Clear Canvas")
                    .min_size(vec2(48.0, 30.0))
                    .ui(ui)
                    .clicked()
                {
                    self.db = DB::default();
                    self.hovered = None;
                    self.selected.clear();
                    self.drag = None;
                    self.connection_manager = ConnectionManager::new(&self.db);
                    self.simulator = Simulator::new();
                }
            });
    }

    fn draw_panel_button(&mut self, ui: &mut Ui, kind: InstanceKind) -> Response {
        let resp = match kind {
            InstanceKind::Gate(gate_kind) => {
                let s = get_icon(ui, gate_kind.graphics().svg.clone())
                    .max_height(PANEL_BUTTON_MAX_HEIGHT);
                ui.add(egui::ImageButton::new(s).sense(Sense::click_and_drag()))
            }
            InstanceKind::Power => {
                let s = get_icon(
                    ui,
                    Power {
                        pos: Pos2::ZERO,
                        on: true,
                    }
                    .graphics()
                    .svg
                    .clone(),
                )
                .max_height(PANEL_BUTTON_MAX_HEIGHT);
                ui.add(egui::ImageButton::new(s).sense(Sense::click_and_drag()))
            }
            InstanceKind::Lamp => {
                let s = get_icon(ui, Lamp { pos: Pos2::ZERO }.graphics().svg.clone())
                    .max_height(PANEL_BUTTON_MAX_HEIGHT);
                ui.add(egui::ImageButton::new(s).sense(Sense::click_and_drag()))
            }
            InstanceKind::Clock => {
                let s = get_icon(
                    ui,
                    Clock {
                        pos: Pos2::ZERO,
                        period: 1,
                    }
                    .graphics()
                    .svg
                    .clone(),
                )
                .max_height(PANEL_BUTTON_MAX_HEIGHT);
                ui.add(egui::ImageButton::new(s).sense(Sense::click_and_drag()))
            }
            InstanceKind::Wire => ui.add(
                Button::new("Wire")
                    .sense(Sense::click_and_drag())
                    .min_size(vec2(78.0, 30.0)),
            ),
            InstanceKind::CustomCircuit(i) => ui.add(
                Button::new(format!("Custom {}", i))
                    .sense(Sense::click_and_drag())
                    .min_size(vec2(78.0, 30.0)),
            ),
        };
        let mouse_pos_world = ui
            .ctx()
            .pointer_interact_pos()
            .map(|p| self.screen_to_world(p));

        if resp.drag_started()
            && let Some(pos) = mouse_pos_world
        {
            let id = match kind {
                InstanceKind::Gate(kind) => self.db.new_gate(Gate { pos, kind }),
                InstanceKind::Power => self.db.new_power(Power { pos, on: true }),
                InstanceKind::Wire => self.db.new_wire(Wire::new_at(pos)),
                InstanceKind::Lamp => self.db.new_lamp(Lamp { pos }),
                InstanceKind::Clock => self.db.new_clock(Clock { pos, period: 1 }),
                InstanceKind::CustomCircuit(c) => self.db.new_custom_circuit(Module {
                    pos,
                    definition_index: c,
                }),
            };
            self.drag = Some(Drag::Canvas(crate::drag::CanvasDrag::Single {
                id,
                offset: Vec2::ZERO,
            }));
        }

        let d_pressed = ui.input(|i| i.key_pressed(egui::Key::D));
        if resp.hovered()
            && d_pressed
            && let InstanceKind::CustomCircuit(i) = kind
        {
            let mut ids = Vec::new();
            for (id, m) in &self.db.modules {
                if m.definition_index == i {
                    ids.push(id);
                }
            }
            for id in ids {
                self.delete_instance(id);
            }
            self.db.module_definitions.remove(i);
        }
        ui.add_space(8.0);

        resp
    }

    fn draw_label_button(&mut self, ui: &mut Ui) -> Response {
        let resp = ui.add(
            Button::new("Label")
                .sense(Sense::click_and_drag())
                .min_size(vec2(78.0, 30.0)),
        );

        let mouse_pos_world = ui
            .ctx()
            .pointer_interact_pos()
            .map(|p| self.screen_to_world(p));

        if resp.drag_started()
            && let Some(pos) = mouse_pos_world
        {
            let id = self.db.new_label(Label::new(pos));
            self.drag = Some(Drag::Label {
                id,
                offset: Vec2::ZERO,
            });
        }
        ui.add_space(8.0);

        resp
    }

    fn handle_copy_pasting(&mut self, ui: &Ui, mouse_pos_world: Option<Pos2>) {
        let mut copy_event_detected = false;
        let mut paste_event_detected = false;
        ui.ctx().input(|i| {
            for event in &i.events {
                if matches!(event, egui::Event::Copy) {
                    log::info!("Copy detected");
                    copy_event_detected = true;
                }
                if let egui::Event::Paste(_) = event {
                    // TODO(paste-json): If user pasted json convert it to gates and add it.
                    paste_event_detected = true;
                }
            }
        });

        if copy_event_detected {
            self.copy_to_clipboard();
        }

        if paste_event_detected
            && !self.clipboard.is_empty()
            && let Some(mouse) = mouse_pos_world
        {
            self.paste_from_clipboard(mouse);
            self.current_dirty = true;
        }
    }

    fn handle_deletion(&mut self, ui: &Ui) {
        let bs_pressed = ui.input(|i| i.key_pressed(egui::Key::Backspace));
        let d_pressed = ui.input(|i| i.key_pressed(egui::Key::D));

        if bs_pressed || d_pressed {
            if let Some(id) = self.hovered.take() {
                match id {
                    Hover::Pin(pin) => self.delete_instance(pin.ins),
                    Hover::Instance(instance_id) => self.delete_instance(instance_id),
                }
            } else if self.hovered.is_none() && !self.selected.is_empty() {
                let ids_to_delete: Vec<InstanceId> = self.selected.drain().collect();
                for id in ids_to_delete {
                    self.delete_instance(id);
                }
            }
        }
    }

    pub fn delete_instance(&mut self, id: InstanceId) {
        self.db.instances.remove(id);
        self.db.types.remove(id);
        self.db.gates.remove(id);
        self.db.powers.remove(id);
        self.db.wires.remove(id);
        self.db.lamps.remove(id);
        self.db.clocks.remove(id);
        self.db.modules.remove(id);
        self.db.connections.retain(|c| !c.involves_instance(id));
        self.hovered.take();
        self.drag.take();
        self.selected.remove(&id);

        self.connection_manager.dirty_instances.remove(&id);
        self.connection_manager.rebuild_spatial_index(&self.db);
        self.current_dirty = true;
    }

    pub fn delete_label(&mut self, id: LabelId) {
        self.db.labels.remove(id);
        if self.editing_label == Some(id) {
            self.editing_label = None;
        }
        self.hovered.take();
        self.drag.take();
    }

    fn draw_canvas(&mut self, ui: &mut Ui) {
        let (resp, _painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let canvas_rect = resp.rect;

        // Set clip rectangle to prevent canvas objects from drawing outside canvas bounds
        ui.set_clip_rect(canvas_rect);

        Self::draw_grid(ui, canvas_rect, self.viewport_offset);

        let mouse_clicked_canvas_or_gates = resp.clicked();
        let double_clicked = ui.input(|i| {
            i.pointer
                .button_double_clicked(egui::PointerButton::Primary)
        });
        let mouse_is_visible = resp.contains_pointer();
        let mouse_pos_world = ui
            .ctx()
            .pointer_hover_pos()
            .map(|p| self.screen_to_world(p));

        let mouse_up = ui.input(|i| i.pointer.any_released());
        // To use the canvas clicked we need to set everything on objects. Right now some stuff are
        // on canvas rect
        let mouse_clicked = ui.input(|i| i.pointer.primary_pressed()) && mouse_is_visible;
        let right_released = ui.input(|i| i.pointer.secondary_released());
        let right_down = ui.input(|i| i.pointer.secondary_down());
        let right_clicked = ui.input(|i| i.pointer.secondary_clicked());

        let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter));
        let esc_pressed = ui.input(|i| i.key_released(egui::Key::Escape));

        if right_down && mouse_is_visible && self.hovered.is_none() {
            self.panning = true;
        }
        if right_released || !mouse_is_visible {
            self.panning = false;
        }
        if self.panning {
            self.viewport_offset += ui.input(|i| i.pointer.delta());
        }

        self.handle_copy_pasting(ui, mouse_pos_world);
        self.handle_deletion(ui);

        if let Some(editing_id) = self.editing_label {
            let label = self.db.get_label_mut(editing_id);
            label.text = self.label_edit_buffer.clone();
            if mouse_up || enter_pressed || esc_pressed {
                self.editing_label = None;
                self.label_edit_buffer.clear();
            }
        }

        // Handle double-click on empty canvas to create new label
        if double_clicked
            && self.hovered.is_none()
            && !self.drag_had_movement
            && let Some(mouse) = mouse_pos_world
        {
            let id = self.db.new_label(Label::new(mouse));
            self.editing_label = Some(id);
            self.label_edit_buffer = String::from("Label");
        }

        if let Some(mouse) = mouse_pos_world {
            let instance_dragging = self.drag.is_some();
            self.hovered = self.get_hovered(mouse);

            if mouse_clicked {
                self.handle_drag_start(mouse);
            }

            if instance_dragging {
                self.handle_dragging(ui, mouse);
            }

            if mouse_up && instance_dragging {
                self.handle_drag_end(mouse);

                if self.connection_manager.update_connections(&mut self.db) {
                    // self.current_dirty = true;
                }
            }
        }
        if self.selected.len() == 1 {
            self.highlight_selected_actions(ui, mouse_pos_world, mouse_clicked_canvas_or_gates);
        }

        if mouse_clicked_canvas_or_gates {
            self.selected.clear();
            if let Some(hovered) = self.hovered {
                self.selected.insert(hovered.instance());
            }
        }

        if right_clicked
            && let Some(id) = self.hovered.as_ref().map(|i| i.instance())
            && matches!(self.db.ty(id), InstanceKind::Power)
        {
            let p = self.db.get_power_mut(id);
            p.on = !p.on;
            self.current_dirty = true;
        }

        if self.current_dirty {
            self.simulator.compute(&self.db);
            self.current_dirty = false;
        }

        // Draw world
        // TODO: Remove the clones. We need to modify the selected, hovered things
        for (id, gate) in self.db.gates.clone() {
            self.draw_gate(ui, id, &gate);
        }
        for (id, power) in &self.db.powers.clone() {
            self.draw_power(ui, id, &power);
        }
        for (id, lamp) in &self.db.lamps.clone() {
            self.draw_lamp(ui, id, &lamp);
        }
        for (id, clock) in &self.db.clocks.clone() {
            self.draw_clock(ui, id, &clock);
        }
        for (id, custom_circuit) in &self.db.modules.clone() {
            self.draw_module(ui, id, custom_circuit);
        }
        for (id, wire) in &self.db.wires {
            let has_current = self.is_on(self.db.wire_start(id));
            self.draw_wire(
                ui,
                wire,
                self.hovered
                    .as_ref()
                    .is_some_and(|f| matches!(f, Hover::Instance(_)) && f.instance() == id),
                has_current,
            );
        }
        // Collect labels to avoid borrowing issues
        let labels: Vec<(LabelId, Label)> = self
            .db
            .labels
            .iter()
            .map(|(id, label)| (id, label.clone()))
            .collect();
        for (id, label) in labels {
            self.draw_label(ui, id, &label);
        }

        for c in &self.potential_connections {
            // Highlight the pin that it's going to attach. The stable pin.
            let pin_to_highlight = c.b;
            let p = self.db.pin_position(pin_to_highlight);
            ui.painter().circle_filled(
                p - self.viewport_offset,
                SNAP_THRESHOLD,
                COLOR_POTENTIAL_CONN_HIGHLIGHT,
            );
        }

        if self.drag.is_none() {
            self.highlight_hovered(ui);
        }
        self.draw_selection_highlight(ui);

        // Preview wire branching
        if !self.selected.is_empty()
            && let Some(hovered) = self.hovered
            && let Some(mouse) = mouse_pos_world
            && self.drag.is_none()
            && let Hover::Instance(instance_id) = hovered
            && let Some(split_point) = self.wire_branching_action_point(mouse, instance_id)
        {
            ui.painter().circle_filled(
                split_point - self.viewport_offset,
                PIN_HOVER_THRESHOLD,
                COLOR_HOVER_PIN_TO_WIRE,
            );
        }
    }

    fn draw_grid(ui: &Ui, canvas_rect: Rect, viewport_offset: Vec2) {
        let grid_color = if ui.visuals().dark_mode {
            COLOR_GRID_DARK
        } else {
            COLOR_GRID_LIGHT
        };

        let painter = ui.painter();

        // Draw vertical lines
        let start_x =
            (canvas_rect.left() / GRID_SIZE).floor() * GRID_SIZE - viewport_offset.x % GRID_SIZE;
        let mut x = start_x;
        while x <= canvas_rect.right() {
            if x >= canvas_rect.left() {
                painter.line_segment(
                    [pos2(x, canvas_rect.top()), pos2(x, canvas_rect.bottom())],
                    Stroke::new(1.0, grid_color),
                );
            }
            x += GRID_SIZE;
        }

        // Draw horizontal lines
        let start_y =
            (canvas_rect.top() / GRID_SIZE).floor() * GRID_SIZE - viewport_offset.y % GRID_SIZE;
        let mut y = start_y;
        while y <= canvas_rect.bottom() {
            if y >= canvas_rect.top() {
                painter.line_segment(
                    [pos2(canvas_rect.left(), y), pos2(canvas_rect.right(), y)],
                    Stroke::new(1.0, grid_color),
                );
            }
            y += GRID_SIZE;
        }
    }

    fn draw_instance_graphics<F>(
        &self,
        ui: &mut Ui,
        graphics: &assets::InstanceGraphics,
        screen_center: Pos2,
        highlight_pin: F,
    ) -> Rect
    where
        F: Fn(usize) -> bool,
    {
        let rect = Rect::from_center_size(screen_center, self.canvas_config.base_gate_size);
        draw_icon_canvas(ui, graphics.svg.clone(), rect);

        for (i, pin) in graphics.pins.iter().enumerate() {
            let pin_offset = pin.offset;
            let pin_pos = screen_center + pin_offset;
            let color = match pin.kind {
                assets::PinKind::Input => self.canvas_config.base_input_pin_color,
                assets::PinKind::Output => self.canvas_config.base_output_pin_color,
            };
            ui.painter()
                .circle_filled(pin_pos, self.canvas_config.base_pin_size, color);

            if highlight_pin(i) {
                ui.painter().circle_stroke(
                    pin_pos,
                    self.canvas_config.base_pin_size + 3.0,
                    Stroke::new(2.0, COLOR_PIN_POWERED_OUTLINE),
                );
            }
        }
        rect
    }

    fn set_selected(&mut self, ui: &mut Ui, rect: Rect, id: InstanceId) {
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        if response.clicked() {
            self.selected.clear();
            self.selected.insert(id);
        }
    }

    fn draw_gate(&mut self, ui: &mut Ui, id: InstanceId, gate: &Gate) {
        let screen_center = gate.pos - self.viewport_offset;
        let rect =
            self.draw_instance_graphics(ui, gate.kind.graphics(), screen_center, |pin_index| {
                self.is_on(Pin {
                    ins: id,
                    index: pin_index as u32,
                })
            });
        self.set_selected(ui, rect, id);
    }

    pub fn draw_gate_preview(&self, ui: &mut Ui, gate_kind: GateKind, pos: Pos2) {
        let screen_center = pos - self.viewport_offset;
        self.draw_instance_graphics(ui, gate_kind.graphics(), screen_center, |_| false);
    }

    fn draw_power(&mut self, ui: &mut Ui, id: InstanceId, power: &Power) {
        let screen_center = power.pos - self.viewport_offset;
        let rect = self.draw_instance_graphics(ui, power.graphics(), screen_center, |pin_index| {
            self.is_on(Pin {
                ins: id,
                index: pin_index as u32,
            })
        });
        self.set_selected(ui, rect, id);
    }

    pub fn draw_power_preview(&self, ui: &mut Ui, pos: Pos2) {
        let power = Power { pos, on: true };
        let screen_center = pos - self.viewport_offset;
        self.draw_instance_graphics(ui, power.graphics(), screen_center, |_| false);
    }

    fn draw_lamp(&mut self, ui: &mut Ui, id: InstanceId, lamp: &Lamp) {
        let has_current = self.is_on(self.db.lamp_input(id));
        let screen_center = lamp.pos - self.viewport_offset;

        if has_current {
            let glow_radius = 60.0;
            let gradient_steps = 30;
            for i in 0..gradient_steps {
                let t = i as f32 / gradient_steps as f32;
                let radius = glow_radius * (1.0 - t);
                let alpha = (255.0 * (1.0 - t) * 0.4) as u8;
                ui.painter().circle_filled(
                    screen_center,
                    radius,
                    Color32::from_rgba_unmultiplied(255, 255, 0, alpha),
                );
            }
        }

        let rect = self.draw_instance_graphics(ui, lamp.graphics(), screen_center, |pin_index| {
            self.is_on(Pin {
                ins: id,
                index: pin_index as u32,
            })
        });
        self.set_selected(ui, rect, id);

        if has_current {
            let rect = Rect::from_center_size(screen_center, self.canvas_config.base_gate_size);
            ui.painter().rect_filled(
                rect,
                CornerRadius::default(),
                Color32::from_rgba_unmultiplied(255, 255, 0, 80),
            );
        }
    }

    pub fn draw_lamp_preview(&self, ui: &mut Ui, pos: Pos2) {
        let lamp = Lamp { pos };
        let screen_center = pos - self.viewport_offset;
        self.draw_instance_graphics(ui, lamp.graphics(), screen_center, |_| false);
    }

    fn draw_clock(&mut self, ui: &mut Ui, id: InstanceId, clock: &Clock) {
        let screen_center = clock.pos - self.viewport_offset;

        let rect = self.draw_instance_graphics(ui, clock.graphics(), screen_center, |pin_index| {
            self.is_on(Pin {
                ins: id,
                index: pin_index as u32,
            })
        });

        self.set_selected(ui, rect, id);
    }

    pub fn draw_clock_preview(&self, ui: &mut Ui, pos: Pos2) {
        let clock = Clock { pos, period: 1 };
        let screen_center = pos - self.viewport_offset;
        self.draw_instance_graphics(ui, clock.graphics(), screen_center, |_| false);
    }

    fn draw_module(
        &mut self,
        ui: &mut Ui,
        id: InstanceId,
        custom_circuit: &crate::custom_circuit::Module,
    ) {
        let screen_center = custom_circuit.pos - self.viewport_offset;

        {
            // Get the definition for this custom circuit
            let Some(definition) = self
                .db
                .module_definitions
                .get(custom_circuit.definition_index)
            else {
                return;
            };

            // Draw as a dark blue rectangle with the name
            let rect = Rect::from_center_size(screen_center, self.canvas_config.base_gate_size);
            ui.painter()
                .rect_filled(rect, CornerRadius::default(), egui::Color32::DARK_BLUE);

            // Draw the name
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &definition.name,
                egui::FontId::default(),
                egui::Color32::WHITE,
            );

            // Draw external pins
            for (pin_index, ext_pin) in definition.external_pins.iter().enumerate() {
                let pin_world_pos = custom_circuit.pos + ext_pin.offset;
                let pin_screen_pos = pin_world_pos - self.viewport_offset;

                // Determine pin color based on whether it has current
                let has_current = self.is_on(Pin {
                    ins: id,
                    index: pin_index as u32,
                });

                let pin_color = match ext_pin.kind {
                    crate::assets::PinKind::Input => egui::Color32::LIGHT_GREEN,
                    crate::assets::PinKind::Output => egui::Color32::LIGHT_RED,
                };

                // Draw the pin
                ui.painter().circle_filled(
                    pin_screen_pos,
                    self.canvas_config.base_pin_size,
                    pin_color,
                );

                // Add outline if it has current
                if has_current {
                    ui.painter().circle_stroke(
                        pin_screen_pos,
                        self.canvas_config.base_pin_size + 3.0,
                        egui::Stroke::new(2.0, COLOR_PIN_POWERED_OUTLINE),
                    );
                }
            }

            self.set_selected(ui, rect, id);
        }
    }

    pub fn draw_custom_circuit_preview(&self, ui: &Ui, definition_index: usize, pos: Pos2) {
        let screen_center = pos - self.viewport_offset;

        // Get the definition for this custom circuit
        if let Some(definition) = self.db.module_definitions.get(definition_index) {
            // Draw as a dark blue rectangle with the name
            let rect = Rect::from_center_size(screen_center, self.canvas_config.base_gate_size);
            ui.painter()
                .rect_filled(rect, CornerRadius::default(), egui::Color32::DARK_BLUE);

            // Draw the name
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                &definition.name,
                egui::FontId::default(),
                egui::Color32::WHITE,
            );

            // Draw external pins (no current highlighting in preview)
            for ext_pin in &definition.external_pins {
                let pin_world_pos = pos + ext_pin.offset;
                let pin_screen_pos = pin_world_pos - self.viewport_offset;

                let pin_color = match ext_pin.kind {
                    crate::assets::PinKind::Input => egui::Color32::LIGHT_GREEN,
                    crate::assets::PinKind::Output => egui::Color32::LIGHT_RED,
                };

                // Draw the pin
                ui.painter().circle_filled(
                    pin_screen_pos,
                    self.canvas_config.base_pin_size,
                    pin_color,
                );
            }
        }
    }

    pub fn draw_wire(&self, ui: &Ui, wire: &Wire, hovered: bool, has_current: bool) {
        let mut color = if has_current {
            COLOR_WIRE_POWERED
        } else {
            COLOR_WIRE_IDLE
        };

        if hovered {
            color = COLOR_WIRE_HOVER;
        }

        ui.painter().line_segment(
            [
                wire.start - self.viewport_offset,
                wire.end - self.viewport_offset,
            ],
            Stroke::new(self.canvas_config.wire_thickness, color),
        );
        ui.painter().circle(
            wire.start - self.viewport_offset,
            PIN_HOVER_THRESHOLD / 2.0,
            color,
            Stroke::NONE,
        );
        ui.painter().circle(
            wire.end - self.viewport_offset,
            PIN_HOVER_THRESHOLD / 2.0,
            color,
            Stroke::NONE,
        );
    }

    fn draw_label(&mut self, ui: &mut Ui, id: LabelId, label: &Label) {
        let screen_pos = label.pos - self.viewport_offset;

        let is_editing = matches!(self.editing_label, Some(editing_id) if editing_id == id);

        let text_color = if ui.visuals().dark_mode {
            Color32::WHITE
        } else {
            Color32::BLACK
        };

        if is_editing {
            let text_size = ui
                .painter()
                .layout_no_wrap(
                    self.label_edit_buffer.clone(),
                    egui::FontId::proportional(LABEL_EDIT_TEXT_SIZE),
                    text_color,
                )
                .size();

            let text_edit_size = vec2(text_size.x.max(100.0), text_size.y);
            let rect = Rect::from_center_size(screen_pos, text_edit_size + vec2(8.0, 4.0));

            let text_edit = egui::TextEdit::singleline(&mut self.label_edit_buffer)
                .desired_width(text_edit_size.x)
                .font(egui::FontId::proportional(LABEL_EDIT_TEXT_SIZE));

            ui.put(rect, text_edit).request_focus();
        } else {
            let text_size = ui
                .painter()
                .layout_no_wrap(
                    label.text.clone(),
                    egui::FontId::proportional(LABEL_DISPLAY_TEXT_SIZE),
                    text_color,
                )
                .size();

            let rect = Rect::from_center_size(screen_pos, text_size + vec2(8.0, 4.0));

            let response = ui.allocate_rect(rect, Sense::click());

            ui.painter().text(
                screen_pos,
                egui::Align2::CENTER_CENTER,
                &label.text,
                egui::FontId::proportional(LABEL_DISPLAY_TEXT_SIZE),
                text_color,
            );

            if response.double_clicked() {
                self.editing_label = Some(id);
                self.label_edit_buffer = label.text.clone();
            }
            if response.hovered() && ui.input(|i| i.key_pressed(egui::Key::D)) {
                self.delete_label(id);
            }
        }
    }

    pub fn get_hovered(&self, mouse_pos: Pos2) -> Option<Hover> {
        if let Some(v) = self.drag {
            match v {
                Drag::Canvas(canvas_drag) => match canvas_drag {
                    crate::drag::CanvasDrag::Single { id, offset: _ } => {
                        return Some(Hover::Instance(id));
                    }
                    crate::drag::CanvasDrag::Selected { .. } => {}
                },
                Drag::Resize { id, start } => {
                    let pin = if start {
                        self.db.wire_start(id)
                    } else {
                        self.db.wire_end(id)
                    };
                    return Some(Hover::Pin(pin));
                }
                Drag::PinToWire {
                    source_pin,
                    start_pos: _,
                } => {
                    // Source pin is being dragged
                    return Some(Hover::Pin(source_pin));
                }
                Drag::BranchWire {
                    original_wire_id,
                    split_point: _,
                    start_mouse_pos: _,
                } => {
                    return Some(Hover::Instance(original_wire_id));
                }
                Drag::Selecting { .. } | Drag::Label { .. } => {}
            }
        }
        for selected in &self.selected {
            match self.db.ty(*selected) {
                InstanceKind::Wire => {
                    for pin in self.db.pins_of(*selected) {
                        if self.db.pin_position(pin).distance(mouse_pos) < PIN_HOVER_THRESHOLD {
                            return Some(Hover::Pin(pin));
                        }
                    }
                    let wire = self.db.get_wire(*selected);
                    let dist = wire.dist_to_closest_point_on_line(mouse_pos);
                    if dist < WIRE_HIT_DISTANCE {
                        return Some(Hover::Instance(*selected));
                    }
                }
                InstanceKind::Gate(_)
                | InstanceKind::Power
                | InstanceKind::Lamp
                | InstanceKind::Clock
                | InstanceKind::CustomCircuit(_) => {}
            }
        }

        // First pins
        for (k, _) in &self.db.powers {
            for pin in self.db.pins_of(k) {
                if self.db.pin_position(pin).distance(mouse_pos) < PIN_HOVER_THRESHOLD {
                    return Some(Hover::Pin(pin));
                }
            }
        }

        for (k, _) in &self.db.gates {
            for pin in self.db.pins_of(k) {
                if self.db.pin_position(pin).distance(mouse_pos) < PIN_HOVER_THRESHOLD {
                    return Some(Hover::Pin(pin));
                }
            }
        }

        for (k, _) in &self.db.lamps {
            for pin in self.db.pins_of(k) {
                if self.db.pin_position(pin).distance(mouse_pos) < PIN_HOVER_THRESHOLD {
                    return Some(Hover::Pin(pin));
                }
            }
        }

        for (k, _) in &self.db.clocks {
            for pin in self.db.pins_of(k) {
                if self.db.pin_position(pin).distance(mouse_pos) < PIN_HOVER_THRESHOLD {
                    return Some(Hover::Pin(pin));
                }
            }
        }

        for (k, _) in &self.db.wires {
            for pin in self.db.pins_of(k) {
                if self.is_pin_connected(pin) {
                    continue;
                }
                if self.db.pin_position(pin).distance(mouse_pos) < PIN_HOVER_THRESHOLD {
                    return Some(Hover::Pin(pin));
                }
            }
        }

        for (k, _) in &self.db.modules {
            for pin in self.db.pins_of(k) {
                if self.is_pin_connected(pin) {
                    continue;
                }
                if self.db.pin_position(pin).distance(mouse_pos) < PIN_HOVER_THRESHOLD {
                    return Some(Hover::Pin(pin));
                }
            }
        }

        // Then instances
        for (k, wire) in &self.db.wires {
            let dist = wire.dist_to_closest_point_on_line(mouse_pos);
            if dist < WIRE_HIT_DISTANCE {
                return Some(Hover::Instance(k));
            }
        }
        for (k, power) in &self.db.powers {
            let rect = Rect::from_center_size(power.pos, self.canvas_config.base_gate_size);
            if rect.contains(mouse_pos) {
                return Some(Hover::Instance(k));
            }
        }
        for (k, gate) in &self.db.gates {
            let rect = Rect::from_center_size(gate.pos, self.canvas_config.base_gate_size);
            if rect.contains(mouse_pos) {
                return Some(Hover::Instance(k));
            }
        }
        for (k, lamp) in &self.db.lamps {
            let rect = Rect::from_center_size(lamp.pos, self.canvas_config.base_gate_size);
            if rect.contains(mouse_pos) {
                return Some(Hover::Instance(k));
            }
        }
        for (k, clock) in &self.db.clocks {
            let rect = Rect::from_center_size(clock.pos, self.canvas_config.base_gate_size);
            if rect.contains(mouse_pos) {
                return Some(Hover::Instance(k));
            }
        }
        for (k, module) in &self.db.modules {
            let rect = Rect::from_center_size(module.pos, self.canvas_config.base_gate_size);
            if rect.contains(mouse_pos) {
                return Some(Hover::Instance(k));
            }
        }
        for (k, wire) in &self.db.wires {
            let dist = wire.dist_to_closest_point_on_line(mouse_pos);
            if dist < WIRE_HIT_DISTANCE {
                return Some(Hover::Instance(k));
            }
        }

        None
    }

    fn highlight_hovered(&self, ui: &Ui) {
        let Some(hovered) = self.hovered else {
            return;
        };

        match hovered {
            Hover::Pin(pin) => {
                let color = COLOR_HOVER_PIN_TO_WIRE;
                let pin_pos = self.db.pin_position(pin) - self.viewport_offset;
                ui.painter()
                    .circle_filled(pin_pos, PIN_HOVER_THRESHOLD, color);
            }
            Hover::Instance(hovered) => match self.db.ty(hovered) {
                InstanceKind::Gate(_) => {
                    let gate = self.db.get_gate(hovered);
                    let outer = Rect::from_center_size(
                        gate.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        outer,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_TICKNESS, COLOR_HOVER_INSTANCE_OUTLINE),
                        StrokeKind::Middle,
                    );
                }
                InstanceKind::Power => {
                    let power = self.db.get_power(hovered);
                    let outer = Rect::from_center_size(
                        power.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        outer,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_TICKNESS, COLOR_HOVER_INSTANCE_OUTLINE),
                        StrokeKind::Middle,
                    );
                }
                InstanceKind::Lamp => {
                    let lamp = self.db.get_lamp(hovered);
                    let outer = Rect::from_center_size(
                        lamp.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        outer,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_TICKNESS, COLOR_HOVER_INSTANCE_OUTLINE),
                        StrokeKind::Middle,
                    );
                }
                InstanceKind::Clock => {
                    let clock = self.db.get_clock(hovered);
                    let outer = Rect::from_center_size(
                        clock.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        outer,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_TICKNESS, COLOR_HOVER_INSTANCE_OUTLINE),
                        StrokeKind::Middle,
                    );
                }
                // Wire is highlighted when drawing
                InstanceKind::Wire => {}
                InstanceKind::CustomCircuit(_) => {
                    let cc = self.db.get_custom_circuit(hovered);
                    let outer = Rect::from_center_size(
                        cc.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        outer,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_TICKNESS, COLOR_HOVER_INSTANCE_OUTLINE),
                        StrokeKind::Middle,
                    );
                }
            },
        }
    }

    pub fn is_pin_connected(&self, pin: Pin) -> bool {
        self.db.connections.iter().any(|c| c.a == pin || c.b == pin)
    }

    fn draw_selection_highlight(&self, ui: &Ui) {
        for &id in &self.selected {
            match self.db.ty(id) {
                InstanceKind::Gate(_) => {
                    let g = self.db.get_gate(id);
                    let r = Rect::from_center_size(
                        g.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_TICKNESS, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
                InstanceKind::Power => {
                    let p = self.db.get_power(id);
                    let r = Rect::from_center_size(
                        p.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_TICKNESS, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
                InstanceKind::Lamp => {
                    let l = self.db.get_lamp(id);
                    let r = Rect::from_center_size(
                        l.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_TICKNESS, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
                InstanceKind::Clock => {
                    let c = self.db.get_clock(id);
                    let r = Rect::from_center_size(
                        c.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_TICKNESS, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
                InstanceKind::Wire => {
                    for pin in self.db.pins_of(id) {
                        let pos = self.db.pin_position(pin);
                        ui.painter().circle_filled(
                            pos - self.viewport_offset,
                            PIN_MOVE_HINT_D,
                            PIN_MOVE_HINT_COLOR,
                        );
                    }
                }
                InstanceKind::CustomCircuit(_) => {
                    let cc = self.db.get_custom_circuit(id);
                    let r = Rect::from_center_size(
                        cc.pos - self.viewport_offset,
                        self.canvas_config.base_gate_size + INSTANEC_OUTLINE,
                    );
                    ui.painter().rect_stroke(
                        r,
                        CornerRadius::default(),
                        Stroke::new(INSTANEC_OUTLINE_TICKNESS, COLOR_SELECTION_HIGHLIGHT),
                        StrokeKind::Outside,
                    );
                }
            }
        }
    }

    fn debug_string(&self, ui: &Ui) -> String {
        let mut out = String::new();
        writeln!(
            out,
            "counts: gates={}, powers={}, lamps={}, clocks={}, wires={}, custom_circuits={}, custom_defs={}, conns={}",
            self.db.gates.len(),
            self.db.powers.len(),
            self.db.lamps.len(),
            self.db.clocks.len(),
            self.db.wires.len(),
            self.db.modules.len(),
            self.db.module_definitions.len(),
            self.db.connections.len()
        )
        .ok();
        let mouse_pos_world = ui
            .ctx()
            .pointer_interact_pos()
            .map(|p| self.screen_to_world(p));
        writeln!(out, "mouse: {mouse_pos_world:?}").ok();

        writeln!(out, "hovered: {:?}", self.hovered).ok();
        writeln!(out, "drag: {:?}", self.drag).ok();
        writeln!(out, "viewport_offset: {:?}", self.viewport_offset).ok();
        writeln!(out, "potential_conns: {}", self.potential_connections.len()).ok();
        writeln!(out, "clipboard: {:?}", self.clipboard).ok();
        writeln!(out, "selected: {:?}", self.selected).ok();
        writeln!(out, "editing_label: {:?}", self.editing_label).ok();
        writeln!(out, "label_edit_buffer: {}", self.label_edit_buffer).ok();

        // Simulation status
        writeln!(out, "\n=== Simulation Status ===").ok();
        writeln!(out, "needs update {}", self.current_dirty).ok();
        match self.simulator.status {
            SimulationStatus::Stable { iterations } => {
                writeln!(out, "Status: STABLE (after {iterations} iterations)").ok();
            }
            SimulationStatus::Unstable { max_reached } => {
                if max_reached {
                    let iters = self.simulator.last_iterations;
                    writeln!(out, "Status: UNSTABLE (max iterations: {iters})").ok();
                } else {
                    writeln!(out, "Status: UNSTABLE").ok();
                }
            }
            SimulationStatus::Running => {
                writeln!(out, "Status: RUNNING...").ok();
            }
        }
        let iters = self.simulator.last_iterations;
        writeln!(out, "Iterations: {iters}").ok();

        // Clock controller state
        writeln!(out, "\n--- Clock Controller ---").ok();
        writeln!(out, "State: {:?}", self.clock_controller.state).ok();
        writeln!(
            out,
            "Tick interval: {:.2}s",
            self.clock_controller.tick_interval
        )
        .ok();
        writeln!(
            out,
            "Tick accumulator: {:.3}s",
            self.clock_controller.tick_accumulator
        )
        .ok();
        writeln!(out, "Voltage: {}", self.clock_controller.voltage).ok();

        writeln!(out, "\nGates:").ok();
        for (id, g) in &self.db.gates {
            writeln!(out, "  {}", g.display(&self.db)).ok();
            // pins
            for (i, pin) in g.kind.graphics().pins.iter().enumerate() {
                let pin_offset = pin.offset;
                let p = g.pos + pin_offset;
                let pin_instance = Pin {
                    ins: id,
                    index: i as u32,
                };
                writeln!(
                    out,
                    "    {} at ({:.1},{:.1})",
                    pin_instance.display(&self.db),
                    p.x,
                    p.y
                )
                .ok();
            }
        }

        writeln!(out, "\nPowers:").ok();
        for (id, p) in &self.db.powers {
            writeln!(out, "  {}", p.display(&self.db)).ok();
            for (i, pin) in p.graphics().pins.iter().enumerate() {
                let pin_offset = pin.offset;
                let pp = p.pos + pin_offset;
                let pin_instance = Pin {
                    ins: id,
                    index: i as u32,
                };
                writeln!(
                    out,
                    "    {} at ({:.1},{:.1})",
                    pin_instance.display(&self.db),
                    pp.x,
                    pp.y
                )
                .ok();
            }
        }

        writeln!(out, "\nLamps:").ok();
        for (id, lamp) in &self.db.lamps {
            writeln!(out, "  {}", lamp.display(&self.db)).ok();
            // Show pins
            for (i, pin) in lamp.graphics().pins.iter().enumerate() {
                let pin_offset = pin.offset;
                let p = lamp.pos + pin_offset;
                let pin_instance = Pin {
                    ins: id,
                    index: i as u32,
                };
                writeln!(
                    out,
                    "    {} at ({:.1},{:.1})",
                    pin_instance.display(&self.db),
                    p.x,
                    p.y
                )
                .ok();
            }
        }

        writeln!(out, "\nClocks:").ok();
        for (id, clock) in &self.db.clocks {
            writeln!(out, "  {}", clock.display(&self.db)).ok();
            // Show pins
            for (i, pin) in clock.graphics().pins.iter().enumerate() {
                let pin_offset = pin.offset;
                let p = clock.pos + pin_offset;
                let pin_instance = Pin {
                    ins: id,
                    index: i as u32,
                };
                writeln!(
                    out,
                    "    {} at ({:.1},{:.1})",
                    pin_instance.display(&self.db),
                    p.x,
                    p.y
                )
                .ok();
            }
        }

        writeln!(out, "\nWires:").ok();
        for (_id, w) in &self.db.wires {
            writeln!(out, "  {}", w.display(&self.db)).ok();
        }

        writeln!(out, "\nModules:").ok();
        for (id, m) in &self.db.modules {
            writeln!(out, "  {}", m.display(&self.db, id)).ok();
        }

        writeln!(out, "\nConnections:").ok();
        for c in &self.db.connections {
            writeln!(out, "  {}", c.display(&self.db)).ok();
        }

        if self.potential_connections.is_empty() {
            writeln!(out, "\nPotential Connections: none").ok();
        } else {
            writeln!(out, "\nPotential Connections:").ok();
            for c in &self.potential_connections {
                writeln!(out, "  {}", c.display(&self.db)).ok();
            }
        }

        writeln!(out, "\n{}", self.connection_manager.debug_info()).ok();

        out
    }

    pub fn extract_instances_with_offsets(
        &self,
        instances: &HashSet<InstanceId>,
    ) -> (Rect, Vec<ClipBoardItem>) {
        let mut points = vec![];
        for &id in instances {
            match self.db.ty(id) {
                InstanceKind::Gate(_) => {
                    let g = self.db.get_gate(id);
                    points.push(g.pos);
                }
                InstanceKind::Power => {
                    let p = self.db.get_power(id);
                    points.push(p.pos);
                }
                InstanceKind::Wire => {
                    let w = self.db.get_wire(id);
                    points.push(w.start);
                    points.push(w.end);
                }
                InstanceKind::Lamp => {
                    let l = self.db.get_lamp(id);
                    points.push(l.pos);
                }
                InstanceKind::Clock => {
                    let c = self.db.get_clock(id);
                    points.push(c.pos);
                }
                InstanceKind::CustomCircuit(_) => {
                    let cc = self.db.get_custom_circuit(id);
                    points.push(cc.pos);
                }
            }
        }
        let rect = Rect::from_points(&points);
        let center = rect.center();

        let mut object_pos = vec![];

        for &id in instances {
            let ty = self.db.ty(id);
            match ty {
                InstanceKind::Gate(kind) => {
                    let g = self.db.get_gate(id);
                    object_pos.push(ClipBoardItem::Gate(kind, center - g.pos));
                }
                InstanceKind::Power => {
                    let p = self.db.get_power(id);
                    object_pos.push(ClipBoardItem::Power(center - p.pos));
                }
                InstanceKind::Wire => {
                    let w = self.db.get_wire(id);
                    object_pos.push(ClipBoardItem::Wire(center - w.start, center - w.end));
                }
                InstanceKind::Lamp => {
                    let l = self.db.get_lamp(id);
                    object_pos.push(ClipBoardItem::Lamp(center - l.pos));
                }
                InstanceKind::Clock => {
                    let c = self.db.get_clock(id);
                    object_pos.push(ClipBoardItem::Clock(center - c.pos));
                }
                InstanceKind::CustomCircuit(_) => {
                    let cc = self.db.get_custom_circuit(id);
                    object_pos.push(ClipBoardItem::CustomCircuit(
                        cc.definition_index,
                        center - cc.pos,
                    ));
                }
            }
        }

        (rect, object_pos)
    }

    fn copy_to_clipboard(&mut self) {
        let instances = if self.selected.is_empty() {
            if let Some(hovered) = self.hovered {
                &HashSet::from_iter(vec![hovered.instance()])
            } else {
                &HashSet::new()
            }
        } else {
            &self.selected
        };
        if instances.is_empty() {
            return;
        }
        let (_, object_pos) = self.extract_instances_with_offsets(instances);
        self.clipboard = object_pos;
    }

    fn paste_from_clipboard(&mut self, mouse: Pos2) {
        self.selected.clear();
        for to_paste in self.clipboard.clone() {
            match to_paste {
                ClipBoardItem::Gate(gate_kind, offset) => {
                    let id = self.db.new_gate(Gate {
                        kind: gate_kind,
                        pos: mouse - offset,
                    });
                    self.connection_manager.mark_instance_dirty(id);
                    self.selected.insert(id);
                }
                ClipBoardItem::Power(offset) => {
                    let id = self.db.new_power(Power {
                        pos: mouse - offset,
                        on: false,
                    });
                    self.connection_manager.mark_instance_dirty(id);
                    self.selected.insert(id);
                }
                ClipBoardItem::Wire(s, e) => {
                    let id = self.db.new_wire(Wire::new(mouse - s, mouse - e));
                    self.connection_manager.mark_instance_dirty(id);
                    self.selected.insert(id);
                }
                ClipBoardItem::CustomCircuit(def_index, offset) => {
                    let id = self.db.new_custom_circuit(custom_circuit::Module {
                        pos: mouse - offset,
                        definition_index: def_index,
                    });
                    self.connection_manager.mark_instance_dirty(id);
                    self.selected.insert(id);
                }
                ClipBoardItem::Lamp(offset) => {
                    let id = self.db.new_lamp(Lamp {
                        pos: mouse - offset,
                    });
                    self.selected.insert(id);
                }
                ClipBoardItem::Clock(offset) => {
                    let id = self.db.new_clock(Clock {
                        pos: mouse - offset,
                        period: 1,
                    });
                    self.selected.insert(id);
                }
                ClipBoardItem::Label(text, offset) => {
                    let _id = self.db.new_label(Label {
                        pos: mouse - offset,
                        text,
                    });
                }
            }
        }
        self.connection_manager.rebuild_spatial_index(&self.db);
        self.current_dirty = true;
    }

    fn highlight_selected_actions(&mut self, ui: &Ui, mouse: Option<Pos2>, mouse_down: bool) {
        let Some(selected) = self.selected.iter().next() else {
            return;
        };
        let selected = *selected;

        match self.db.ty(selected) {
            InstanceKind::Wire => {
                for pin in self.db.pins_of(selected) {
                    let pos = self.db.pin_position(pin);
                    ui.painter().circle_filled(
                        pos - self.viewport_offset,
                        PIN_MOVE_HINT_D,
                        PIN_MOVE_HINT_COLOR,
                    );

                    if let Some(mouse) = mouse
                        && mouse_down
                        && mouse.distance(pos) < PIN_MOVE_HINT_D
                    {
                        self.drag = Some(Drag::Resize {
                            id: selected,
                            start: pin.index == 0,
                        });
                    }
                }
            }
            InstanceKind::Gate(_)
            | InstanceKind::Power
            | InstanceKind::Lamp
            | InstanceKind::Clock
            | InstanceKind::CustomCircuit(_) => {}
        }
    }

    pub fn split_wire_at_point(&mut self, wire_id: InstanceId, split_point: Pos2) {
        let original_wire = *self.db.get_wire(wire_id);

        let new_wire = Wire::new(split_point, original_wire.end);
        let new_wire_id = self.db.new_wire(new_wire);

        let original_wire_mut = self.db.get_wire_mut(wire_id);
        original_wire_mut.end = split_point;

        self.connection_manager
            .mark_instances_dirty(&[wire_id, new_wire_id]);
    }

    pub fn wire_branching_action_point(
        &self,
        mouse: Pos2,
        instance_id: InstanceId,
    ) -> Option<Pos2> {
        if !self.selected.contains(&instance_id) || self.selected.len() != 1 {
            return None;
        }
        if !matches!(self.db.ty(instance_id), InstanceKind::Wire) {
            return None;
        }
        let wire = self.db.get_wire(instance_id);

        if wire.dist_to_closest_point_on_line(mouse) > NEW_PIN_ON_WIRE_THRESHOLD {
            return None;
        }

        let split_point = wire.closest_point_on_line(mouse);
        if (split_point - wire.start).length() < MIN_WIRE_SIZE
            || (split_point - wire.end).length() < MIN_WIRE_SIZE
        {
            return None;
        }

        Some(split_point)
    }

    fn create_module(&mut self) {
        // Generate a unique name for the custom circuit
        let circuit_name = format!("module {}", self.db.module_definitions.len() + 1);

        match self.create_custom_circuit(circuit_name, &self.selected.clone()) {
            Ok(()) => {
                log::info!("Custom circuit created successfully");
                self.selected.clear();
            }
            Err(e) => {
                log::error!("Failed to create custom circuit: {e}");
            }
        }
    }
}

fn get_icon<'a>(ui: &Ui, source: egui::ImageSource<'a>) -> Image<'a> {
    let mut image = egui::Image::new(source);

    if ui.visuals().dark_mode {
        image = image.bg_fill(Color32::WHITE);
    }

    image
}

fn draw_icon_canvas(ui: &mut Ui, source: egui::ImageSource<'_>, rect: Rect) {
    let image = get_icon(ui, source).fit_to_exact_size(rect.size());

    ui.put(rect, image);
}
