use std::collections::HashSet;
use std::fmt::Display;
use std::hash::Hash;

use egui::{Pos2, Vec2, pos2};
use slotmap::{SecondaryMap, SlotMap};

use crate::assets::PinKind;
use crate::{
    assets::{self},
    config::CanvasConfig,
    connection_manager::Connection,
    module::{Module, ModuleDefinition},
};

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

impl From<u32> for LabelId {
    fn from(value: u32) -> Self {
        Self(slotmap::KeyData::from_ffi(value as u64))
    }
}

slotmap::new_key_type! {
    pub struct ModuleDefId;
}

impl From<u32> for ModuleDefId {
    fn from(value: u32) -> Self {
        Self(slotmap::KeyData::from_ffi(value as u64))
    }
}

#[derive(Default, serde::Deserialize, serde::Serialize, Debug, Clone)]
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
    // Definition of modules created by the user
    pub module_definitions: SlotMap<ModuleDefId, ModuleDefinition>,
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

    pub fn new_module(&mut self, c: crate::module::Module) -> InstanceId {
        let k = self.instances.insert(());
        let definition_index = c.definition_index;
        self.modules.insert(k, c);
        self.types.insert(k, InstanceKind::Module(definition_index));
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

    pub fn get_module(&self, id: InstanceId) -> &Module {
        self.modules.get(id).expect("module not found")
    }

    pub fn get_module_mut(&mut self, id: InstanceId) -> &mut Module {
        self.modules.get_mut(id).expect("modules not found (mut)")
    }

    pub fn get_module_def(&self, def_index: ModuleDefId) -> &ModuleDefinition {
        self.module_definitions
            .get(def_index)
            .expect("module def not found")
    }

    // Pin helper methods with type checking

    // Gates - generic versions
    pub fn gate_inp_n(&self, id: InstanceId, n: u32) -> Pin {
        self.get_gate(id); // Type check
        assert!(n < 2, "Gates only have 2 inputs (0 and 1)");
        Pin::new(id, if n == 0 { 0 } else { 2 }, PinKind::Input)
    }

    pub fn gate_output_n(&self, id: InstanceId, n: u32) -> Pin {
        self.get_gate(id); // Type check
        assert!(n == 0, "Gates only have 1 output");
        Pin::new(id, 1, PinKind::Output)
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
        Pin::new(
            id,
            n,
            if n == 0 {
                PinKind::Input
            } else {
                PinKind::Output
            },
        )
    }

    pub fn wire_start(&self, id: InstanceId) -> Pin {
        self.wire_pin_n(id, 0)
    }

    pub fn wire_end(&self, id: InstanceId) -> Pin {
        self.wire_pin_n(id, 1)
    }

    pub fn power_output(&self, id: InstanceId) -> Pin {
        self.get_power(id);
        Pin::new(id, 0, PinKind::Output)
    }

    pub fn lamp_input(&self, id: InstanceId) -> Pin {
        self.get_lamp(id);
        Pin::new(id, 0, PinKind::Input)
    }

    pub fn clock_output(&self, id: InstanceId) -> Pin {
        self.get_clock(id);
        Pin::new(id, 0, PinKind::Output)
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

    pub fn gate_ids(&self) -> Vec<InstanceId> {
        self.gates.keys().collect()
    }

    pub fn power_ids(&self) -> Vec<InstanceId> {
        self.powers.keys().collect()
    }

    pub fn lamp_ids(&self) -> Vec<InstanceId> {
        self.lamps.keys().collect()
    }

    pub fn clock_ids(&self) -> Vec<InstanceId> {
        self.clocks.keys().collect()
    }

    pub fn module_ids(&self) -> Vec<InstanceId> {
        self.modules.keys().collect()
    }

    pub fn wire_ids(&self) -> Vec<InstanceId> {
        self.wires.keys().collect()
    }

    pub fn label_ids(&self) -> Vec<LabelId> {
        self.labels.keys().collect()
    }

    pub fn pins_of(&self, id: InstanceId) -> Vec<Pin> {
        match self.ty(id) {
            InstanceKind::Gate(gk) => {
                let graphics = gk.graphics();
                graphics
                    .pins
                    .iter()
                    .enumerate()
                    .map(|(i, p)| Pin::new(id, i as u32, p.kind))
                    .collect()
            }
            InstanceKind::Power => {
                let graphics = assets::POWER_OFF_GRAPHICS.clone();
                graphics
                    .pins
                    .iter()
                    .enumerate()
                    .map(|(i, p)| Pin::new(id, i as u32, p.kind))
                    .collect()
            }
            InstanceKind::Wire => {
                let wire = self.get_wire(id);
                vec![
                    Pin::new(
                        id,
                        0,
                        if wire.input_index == 0 {
                            assets::PinKind::Input
                        } else {
                            assets::PinKind::Output
                        },
                    ),
                    Pin::new(
                        id,
                        1,
                        if wire.input_index == 1 {
                            assets::PinKind::Input
                        } else {
                            assets::PinKind::Output
                        },
                    ),
                ]
            }
            InstanceKind::Lamp => {
                let graphics = assets::LAMP_GRAPHICS.clone();
                graphics
                    .pins
                    .iter()
                    .enumerate()
                    .map(|(i, p)| Pin::new(id, i as u32, p.kind))
                    .collect()
            }
            InstanceKind::Clock => {
                let graphics = assets::CLOCK_GRAPHICS.clone();
                graphics
                    .pins
                    .iter()
                    .enumerate()
                    .map(|(i, p)| Pin::new(id, i as u32, p.kind))
                    .collect()
            }
            InstanceKind::Module(_) => {
                // TODO: Modules
                vec![]
            }
        }
    }

    pub fn pin_position(&self, pin: Pin, canvas_config: &CanvasConfig) -> Pos2 {
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
            InstanceKind::Module(_) => {
                let cc = self.get_module(pin.ins);
                cc.pos + self.pin_offset(pin, canvas_config)
            }
        }
    }

    pub fn pin_offset(&self, pin: Pin, canvas_config: &CanvasConfig) -> Vec2 {
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
            InstanceKind::Module(_) => {
                // TODO: Modules
                Vec2::ZERO
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
                InstanceKind::Module(_) => {
                    let cc = self.get_module_mut(*id);
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

    pub fn move_instance_and_propagate(
        &mut self,
        id: InstanceId,
        delta: Vec2,
        canvas_config: &CanvasConfig,
    ) {
        let mut visited = HashSet::new();
        self.move_instance_and_propagate_recursive(id, delta, &mut visited, canvas_config);
    }

    fn move_instance_and_propagate_recursive(
        &mut self,
        id: InstanceId,
        delta: Vec2,
        visited: &mut HashSet<InstanceId>,
        canvas_config: &CanvasConfig,
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
            InstanceKind::Module(_) => {
                let cc = self.get_module_mut(id);
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
                                let new_pin_pos = self.pin_position(moved_pin, canvas_config);
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
                | InstanceKind::Module(_) => {
                    // For non-wires, propagate the same delta
                    self.move_instance_and_propagate_recursive(
                        connected_id,
                        delta,
                        visited,
                        canvas_config,
                    );
                }
            }
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub enum InstanceKind {
    Gate(GateKind),
    Power,
    Wire,
    Lamp,
    Clock,
    Module(ModuleDefId),
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

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Gate {
    // Center position
    pub pos: Pos2,
    pub kind: GateKind,
}

impl GateKind {
    pub fn graphics(&self) -> assets::InstanceGraphics {
        match self {
            Self::Nand => assets::NAND_GRAPHICS.clone(),
            Self::And => assets::AND_GRAPHICS.clone(),
            Self::Or => assets::OR_GRAPHICS.clone(),
            Self::Nor => assets::NOR_GRAPHICS.clone(),
            Self::Xor => assets::XOR_GRAPHICS.clone(),
            Self::Xnor => assets::XNOR_GRAPHICS.clone(),
        }
    }
}

impl Gate {
    pub fn display(&self, db: &DB) -> String {
        // Find the InstanceId for this gate in the database
        for (id, gate) in &db.gates {
            if gate.pos == self.pos && gate.kind == self.kind {
                return format!("{:?} {}", self.kind, id);
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

    pub fn graphics(&self) -> assets::InstanceGraphics {
        if self.on {
            assets::POWER_ON_GRAPHICS.clone()
        } else {
            assets::POWER_OFF_GRAPHICS.clone()
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

    pub fn graphics(&self) -> assets::InstanceGraphics {
        assets::LAMP_GRAPHICS.clone()
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

    pub fn graphics(&self) -> assets::InstanceGraphics {
        assets::CLOCK_GRAPHICS.clone()
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
    pub input_index: u32,
}

impl Wire {
    pub fn display(&self, db: &DB) -> String {
        for (id, wire) in &db.wires {
            if wire.start == self.start
                && wire.end == self.end
                && wire.input_index == self.input_index
            {
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
        Self {
            start,
            end,
            input_index: 0,
        }
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

// A specific pin on an instance
#[derive(
    serde::Deserialize, serde::Serialize, Copy, Debug, Clone, Eq, PartialEq, Hash, Ord, PartialOrd,
)]
pub struct Pin {
    pub ins: InstanceId,
    pub index: u32,
    pub kind: PinKind,
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
            InstanceKind::Module(_) => db.get_module(self.ins).display(db, self.ins),
        };
        format!(
            "{:?} pin#{} in {} ",
            self.kind, self.index, instance_display,
        )
    }

    pub fn display_alone(&self) -> String {
        format!("pin#{} {:?}", self.index, self.kind)
    }

    pub fn new(ins: InstanceId, index: u32, kind: PinKind) -> Self {
        Self { ins, index, kind }
    }
}
