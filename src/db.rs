use std::collections::HashSet;
use std::fmt::Display;
use std::hash::Hash;

use egui::{Pos2, Vec2, pos2};
use slotmap::{SecondaryMap, SlotMap};

use crate::assets::PinKind;
use crate::connection_manager::ConnectionKind;
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

impl Display for ModuleDefId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&format!("{:?}", self.0))
    }
}

#[derive(Default, serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Circuit {
    // Type registry for each instance id
    pub types: SlotMap<InstanceId, InstanceKind>,
    // Per-kind payloads keyed off the primary key space
    pub gates: SecondaryMap<InstanceId, Gate>,
    pub powers: SecondaryMap<InstanceId, Power>,
    pub wires: SecondaryMap<InstanceId, Wire>,
    pub lamps: SecondaryMap<InstanceId, Lamp>,
    pub clocks: SecondaryMap<InstanceId, Clock>,
    pub modules: SecondaryMap<InstanceId, Module>,
    pub connections: HashSet<Connection>,
    // Labels
    pub labels: SlotMap<LabelId, Label>,
}
impl Circuit {
    pub fn ty(&self, id: InstanceId) -> InstanceKind {
        self.types
            .get(id)
            .copied()
            .unwrap_or_else(|| panic!("instance type missing for id: {id:?}"))
    }

    /// Remove a single instance without cascade deletion
    pub(crate) fn remove_single_instance(&mut self, id: InstanceId) {
        match self.ty(id) {
            InstanceKind::Gate(_) => {
                self.gates.remove(id);
            }
            InstanceKind::Power => {
                self.powers.remove(id);
            }
            InstanceKind::Wire => {
                self.wires.remove(id);
            }
            InstanceKind::Lamp => {
                self.lamps.remove(id);
            }
            InstanceKind::Clock => {
                self.clocks.remove(id);
            }
            InstanceKind::Module(_) => {
                self.modules.remove(id);
            }
        };
        self.types.remove(id);
        self.connections.retain(|c| !c.involves_instance(id));
    }

    pub fn new_gate(&mut self, g: Gate) -> InstanceId {
        let k = self.types.insert(InstanceKind::Gate(g.kind));
        self.gates.insert(k, g);
        k
    }

    pub fn new_power(&mut self, p: Power) -> InstanceId {
        let k = self.types.insert(InstanceKind::Power);
        self.powers.insert(k, p);
        k
    }

    pub fn new_wire(&mut self, w: Wire) -> InstanceId {
        let k = self.types.insert(InstanceKind::Wire);
        self.wires.insert(k, w);
        k
    }

    pub fn new_lamp(&mut self, l: Lamp) -> InstanceId {
        let k = self.types.insert(InstanceKind::Lamp);
        self.lamps.insert(k, l);
        k
    }

    pub fn new_clock(&mut self, c: Clock) -> InstanceId {
        let k = self.types.insert(InstanceKind::Clock);
        self.clocks.insert(k, c);
        k
    }

    pub fn new_module_id(&mut self, m: crate::module::Module) -> InstanceId {
        let k = self.types.insert(InstanceKind::Module(m.definition_id));
        self.modules.insert(k, m);
        k
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

    pub fn display(&self, db: &DB, simulator: Option<&crate::simulator::Simulator>) -> String {
        let mut out = String::new();
        use std::fmt::Write as _;

        let total = self.types.len();
        writeln!(out, "======================================").ok();
        writeln!(out, "  INSTANCES ({total} total)").ok();
        writeln!(out, "======================================").ok();

        // Collect all instances with their display info
        let instances: Vec<(InstanceId, InstanceKind)> =
            self.types.iter().map(|(id, k)| (id, *k)).collect();
        let instance_count = instances.len();

        for (idx, (id, kind)) in instances.iter().enumerate() {
            let is_last_instance = idx == instance_count - 1;
            let branch = if is_last_instance { "`-" } else { "|-" };
            let cont = if is_last_instance { "   " } else { "|  " };

            // Instance header
            let header = self.instance_header(*id, *kind, db);
            writeln!(out, "{branch} {header}").ok();

            // Get pins for this instance
            let pins = self.pins_of(*id, db);
            let pin_count = pins.len();

            for (pin_idx, pin) in pins.iter().enumerate() {
                let is_last_pin = pin_idx == pin_count - 1;
                let pin_branch = if is_last_pin { "`-" } else { "|-" };

                // Get connections for this pin
                let connected = self.connected_pins(*pin);
                let kind_str = match pin.kind {
                    PinKind::Input => "In",
                    PinKind::Output => "Out",
                };
                let arrow = match pin.kind {
                    PinKind::Input => "<-",
                    PinKind::Output => "->",
                };

                let conn_str = if connected.is_empty() {
                    "(unconnected)".to_owned()
                } else {
                    connected
                        .iter()
                        .map(|p| p.display_short(self, db))
                        .collect::<Vec<_>>()
                        .join(", ")
                };

                // Get pin state if simulator is available
                let state_str = if let Some(sim) = simulator {
                    if let Some(&value) = sim.current.get(pin) {
                        match value {
                            crate::simulator::Value::One => " O",
                            crate::simulator::Value::Zero => " N",
                            crate::simulator::Value::X => " X",
                        }
                    } else {
                        ""
                    }
                } else {
                    ""
                };

                writeln!(
                    out,
                    "{}{} #{} ({})  {} {}{}",
                    cont, pin_branch, pin.index, kind_str, arrow, conn_str, state_str
                )
                .ok();
            }

            // Show instance members for modules with indentation
            if let InstanceKind::Module(_) = kind {
                let module = self.get_module(*id);
                let member_count = module.instance_members.len();
                for (member_idx, member_id) in module.instance_members.iter().enumerate() {
                    let is_last_member = member_idx == member_count - 1;
                    let member_branch = if is_last_member { "`-" } else { "|-" };
                    let member_cont = if is_last_member {
                        format!("{cont}   ")
                    } else {
                        format!("{cont}|  ")
                    };

                    // Member header
                    let member_kind = self.ty(*member_id);
                    let member_header = self.instance_header(*member_id, member_kind, db);
                    writeln!(out, "{cont}{member_branch} {member_header}").ok();

                    // Get pins for this member
                    let member_pins = self.pins_of(*member_id, db);
                    let member_pin_count = member_pins.len();

                    for (member_pin_idx, member_pin) in member_pins.iter().enumerate() {
                        let is_last_member_pin = member_pin_idx == member_pin_count - 1;
                        let member_pin_branch = if is_last_member_pin { "`-" } else { "|-" };

                        // Get connections for this member pin
                        let member_connected = self.connected_pins(*member_pin);
                        let member_kind_str = match member_pin.kind {
                            PinKind::Input => "In",
                            PinKind::Output => "Out",
                        };
                        let member_arrow = match member_pin.kind {
                            PinKind::Input => "<-",
                            PinKind::Output => "->",
                        };

                        let member_conn_str = if member_connected.is_empty() {
                            "(unconnected)".to_owned()
                        } else {
                            member_connected
                                .iter()
                                .map(|p| p.display_short(self, db))
                                .collect::<Vec<_>>()
                                .join(", ")
                        };

                        // Get pin state if simulator is available
                        let member_state_str = if let Some(sim) = simulator {
                            if let Some(&value) = sim.current.get(member_pin) {
                                match value {
                                    crate::simulator::Value::One => " O",
                                    crate::simulator::Value::Zero => " N",
                                    crate::simulator::Value::X => " X",
                                }
                            } else {
                                ""
                            }
                        } else {
                            ""
                        };

                        writeln!(
                            out,
                            "{member_cont}{member_pin_branch} #{} ({})  {} {}{}",
                            member_pin.index,
                            member_kind_str,
                            member_arrow,
                            member_conn_str,
                            member_state_str
                        )
                        .ok();
                    }
                }
            }
        }

        writeln!(out).ok();
        writeln!(out, "======================================").ok();
        writeln!(out, "  CONNECTIONS ({} total)", self.connections.len()).ok();
        writeln!(out, "======================================").ok();
        let mut sorted_connections: Vec<_> = self.connections.iter().collect();
        sorted_connections.sort_by_key(|c| {
            let a_is_input = c.a.kind == PinKind::Input;
            let b_is_input = c.b.kind == PinKind::Input;
            (a_is_input, b_is_input)
        });
        for c in sorted_connections {
            writeln!(out, "{}", c.display_short(self, db)).ok();
        }

        out
    }

    /// Short header for an instance (e.g., "AND [0v1]" or "Power [1v1] ON")
    fn instance_header(&self, id: InstanceId, kind: InstanceKind, db: &DB) -> String {
        match kind {
            InstanceKind::Gate(gk) => format!("{gk:?} [{id}]"),
            InstanceKind::Power => {
                let p = self.get_power(id);
                let state = if p.on { "ON" } else { "OFF" };
                format!("Power [{id}] {state}")
            }
            InstanceKind::Wire => format!("Wire [{id}]"),
            InstanceKind::Lamp => format!("Lamp [{id}]"),
            InstanceKind::Clock => format!("Clock [{id}]"),
            InstanceKind::Module(def_id) => {
                let name = db
                    .module_definitions
                    .get(def_id)
                    .map(|d| d.name.as_str())
                    .unwrap_or("?");
                format!("Module \"{name}\" [{id}]")
            }
        }
    }

    // Connections

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

    pub fn connections_containing(&self, pin: Pin) -> Vec<Connection> {
        let mut res = Vec::new();
        for c in &self.connections {
            if c.involves_pin(&pin) {
                res.push(*c);
            }
        }
        res
    }

    pub fn pins_of(&self, id: InstanceId, db: &DB) -> Vec<Pin> {
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
            InstanceKind::Module(def_id) => self.get_module(id).pins(),
        }
    }

    pub fn pin_position(&self, pin: Pin, canvas_config: &CanvasConfig, db: &DB) -> Pos2 {
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
                cc.pos + self.pin_offset(pin, canvas_config, db)
            }
        }
    }

    pub fn pin_offset(&self, pin: Pin, canvas_config: &CanvasConfig, db: &DB) -> Vec2 {
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
            InstanceKind::Module(def_id) => {
                let module_def = db.get_module_def(def_id);
                let module = db.circuit.get_module(pin.ins);
                module_def.calculate_pin_offset(db, &module.pins(), &pin, canvas_config)
            }
        }
    }

    pub fn move_instance_and_propagate(
        &mut self,
        id: InstanceId,
        delta: Vec2,
        canvas_config: &CanvasConfig,
        db: &DB,
    ) {
        let mut visited = HashSet::new();
        self.move_instance_and_propagate_recursive(id, delta, &mut visited, canvas_config, db);
    }

    fn move_instance_and_propagate_recursive(
        &mut self,
        id: InstanceId,
        delta: Vec2,
        visited: &mut HashSet<InstanceId>,
        canvas_config: &CanvasConfig,
        db: &DB,
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
                    let wire_pins = self.pins_of(connected_id, db);
                    for wire_pin in wire_pins {
                        // Check if this wire pin is connected to any pin of our moved instance
                        for moved_pin in self.pins_of(id, db) {
                            if self
                                .connections
                                .contains(&Connection::new(wire_pin, moved_pin))
                            {
                                let new_pin_pos = self.pin_position(moved_pin, canvas_config, db);
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
                        db,
                    );
                }
            }
        }
    }
}

#[derive(Default, serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct DB {
    pub circuit: Circuit,
    // Definition of modules created by the user
    pub module_definitions: SlotMap<ModuleDefId, ModuleDefinition>,
}

impl DB {
    pub fn get_module_def(&self, def_index: ModuleDefId) -> &ModuleDefinition {
        self.module_definitions
            .get(def_index)
            .expect("module def not found")
    }

    /// Returns the parent module ID if this instance is part of a module,
    /// or None if it's a top-level instance.
    pub fn get_module_owner(&self, id: InstanceId) -> Option<InstanceId> {
        for (ins, module) in &self.circuit.modules {
            if module.instance_members.contains(&id) {
                return Some(ins);
            }
        }
        None
    }

    /// Get all instances that belong to a specific module
    pub fn get_instances_for_module(&self, module_id: InstanceId) -> Vec<InstanceId> {
        self.circuit.get_module(module_id).instance_members.clone()
    }

    /// Check if an instance is hidden from UI (i.e., it's part of a module)
    pub fn is_hidden(&self, id: InstanceId) -> bool {
        self.get_module_owner(id).is_some()
    }

    /// Get all instances that are visible in UI (not hidden)
    pub fn visible_instances(&self) -> impl Iterator<Item = InstanceId> + '_ {
        self.circuit.types.keys().filter(|id| !self.is_hidden(*id))
    }

    /// Remove an instance, with cascade deletion for modules
    pub fn remove_instance(&mut self, id: InstanceId) {
        if matches!(self.circuit.ty(id), InstanceKind::Module(_)) {
            let internal_instances = self.get_instances_for_module(id);
            for child_id in internal_instances {
                self.remove_single_instance(child_id);
            }
        }

        self.remove_single_instance(id);
    }

    /// Remove a single instance without cascade deletion
    fn remove_single_instance(&mut self, id: InstanceId) {
        // Remove from circuit
        self.circuit.remove_single_instance(id);
    }

    /// Create a new module instance and automatically flatten its internal components
    pub fn new_module(&mut self, definition_id: ModuleDefId, pos: Pos2) -> InstanceId {
        // TODO: Currently the way module is created is messy. First allocate ID and then flatten
        // the module into circuit and then insert it in the circuit. It should just become one
        // function.
        let module_id = self
            .circuit
            .types
            .insert(InstanceKind::Module(definition_id));
        // TODO: This requires temporarily cloning the definition to avoid borrow checker issues.
        // In the future, we could optimize this with better data structure design.
        let definition = self.get_module_def(definition_id).clone();
        let module_defs = self.module_definitions.clone();
        let module = definition.flatten_into_circuit(definition_id, module_id, pos, self);

        self.circuit.modules.insert(module_id, module);
        module_id
    }

    pub fn move_nonwires_and_resize_wires(&mut self, ids: &[InstanceId], delta: Vec2) {
        let ids_set: HashSet<InstanceId> = ids.iter().copied().collect();

        for id in ids {
            match self.circuit.ty(*id) {
                InstanceKind::Gate(_) => {
                    let g = self.circuit.get_gate_mut(*id);
                    g.pos += delta;
                }
                InstanceKind::Power => {
                    let p = self.circuit.get_power_mut(*id);
                    p.pos += delta;
                }
                InstanceKind::Wire => {
                    let w = self.circuit.get_wire_mut(*id);
                    w.start += delta;
                    w.end += delta;
                }
                InstanceKind::Lamp => {
                    let l = self.circuit.get_lamp_mut(*id);
                    l.pos += delta;
                }
                InstanceKind::Clock => {
                    let c = self.circuit.get_clock_mut(*id);
                    c.pos += delta;
                }
                InstanceKind::Module(_) => {
                    let cc = self.circuit.get_module_mut(*id);
                    cc.pos += delta;
                }
            }
        }

        for id in ids {
            for pin in self.circuit.connected_pins_of_instance(*id) {
                if matches!(self.circuit.ty(pin.ins), InstanceKind::Wire)
                    && !ids_set.contains(&pin.ins)
                {
                    // Otherwise resize the wire
                    let w = self.circuit.get_wire_mut(pin.ins);
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
        match self.circuit.ty(id) {
            InstanceKind::Gate(_) => {
                let g = self.circuit.get_gate_mut(id);
                g.pos += delta;
            }
            InstanceKind::Power => {
                let p = self.circuit.get_power_mut(id);
                p.pos += delta;
            }
            InstanceKind::Wire => {
                let w = self.circuit.get_wire_mut(id);
                w.start += delta;
                w.end += delta;
            }
            InstanceKind::Lamp => {
                let l = self.circuit.get_lamp_mut(id);
                l.pos += delta;
            }
            InstanceKind::Clock => {
                let c = self.circuit.get_clock_mut(id);
                c.pos += delta;
            }
            InstanceKind::Module(_) => {
                let cc = self.circuit.get_module_mut(id);
                cc.pos += delta;
            }
        }

        let connected = self.circuit.connected_insntances(id);

        for connected_id in connected {
            if connected_id == id || visited.contains(&connected_id) {
                continue;
            }

            match self.circuit.ty(connected_id) {
                InstanceKind::Wire => {
                    let wire_pins = self.circuit.pins_of(connected_id, self);
                    for wire_pin in wire_pins {
                        for moved_pin in self.circuit.pins_of(id, self) {
                            if self
                                .circuit
                                .connections
                                .contains(&Connection::new(wire_pin, moved_pin))
                            {
                                let new_pin_pos =
                                    self.circuit.pin_position(moved_pin, canvas_config, self);
                                let w = self.circuit.get_wire_mut(connected_id);
                                if wire_pin.index == 0 {
                                    w.start = new_pin_pos;
                                } else {
                                    w.end = new_pin_pos;
                                }
                            }
                        }
                    }
                    visited.insert(connected_id);
                }
                InstanceKind::Gate(_)
                | InstanceKind::Power
                | InstanceKind::Lamp
                | InstanceKind::Clock
                | InstanceKind::Module(_) => {
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

    fn circuit_of(&self, id: InstanceId) -> &Circuit {
        &self.circuit
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
    Not,
}

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Gate {
    /// pos is the position of gate on the canvas. It's an absolute value. So it needs to be subbed
    /// `viewport_offset` to get the relative position of this object on the screen.
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
            Self::Not => assets::NOT_GRAPHICS.clone(),
        }
    }
}

impl Gate {
    pub fn display(&self, id: InstanceId) -> String {
        format!("{:?} {}", self.kind, id)
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
    pub fn display(&self, id: InstanceId) -> String {
        format!("Power {{ id: {}, on: {}}}", id, self.on)
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
    pub fn graphics(&self) -> assets::InstanceGraphics {
        assets::LAMP_GRAPHICS.clone()
    }

    pub fn display(&self, id: InstanceId) -> String {
        format!("Lamp {id}")
    }
}

// Lamp end

// Clock

#[derive(serde::Deserialize, serde::Serialize, Copy, Debug, Clone)]
pub struct Clock {
    pub pos: Pos2,
}

impl Clock {
    pub fn graphics(&self) -> assets::InstanceGraphics {
        assets::CLOCK_GRAPHICS.clone()
    }

    pub fn display(&self, id: InstanceId) -> String {
        format!("Clock {id}")
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
    pub fn display(&self, id: InstanceId) -> String {
        format!("Wire {id}")
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
    pub fn display(&self, circuit: &Circuit) -> String {
        let instance_display = match circuit.ty(self.ins) {
            InstanceKind::Gate(_) => {
                let gate = circuit.get_gate(self.ins);
                gate.display(self.ins)
            }
            InstanceKind::Power => {
                let power = circuit.get_power(self.ins);
                power.display(self.ins)
            }
            InstanceKind::Wire => {
                let wire = circuit.get_wire(self.ins);
                wire.display(self.ins)
            }
            InstanceKind::Lamp => {
                let lamp = circuit.get_lamp(self.ins);
                lamp.display(self.ins)
            }
            InstanceKind::Clock => {
                let clock = circuit.get_clock(self.ins);
                clock.display(self.ins)
            }
            InstanceKind::Module(_) => format!("Module {}", self.ins),
        };
        format!("{:?} #{} in {} ", self.kind, self.index, instance_display,)
    }

    pub fn pos(&self, db: &DB, canvas_config: &CanvasConfig) -> Pos2 {
        let circuit = db.circuit_of(self.ins);
        circuit.pin_position(*self, canvas_config, db)
    }

    pub fn display_alone(&self) -> String {
        format!("pin#{} {:?}", self.index, self.kind)
    }

    /// Short display for connections: "And[0v1].pin#0"
    pub fn display_short(&self, circuit: &Circuit, db: &DB) -> String {
        let kind = circuit.ty(self.ins);
        let type_name = match kind {
            InstanceKind::Gate(gk) => format!("{gk:?}"),
            InstanceKind::Power => "Power".to_owned(),
            InstanceKind::Wire => "Wire".to_owned(),
            InstanceKind::Lamp => "Lamp".to_owned(),
            InstanceKind::Clock => "Clock".to_owned(),
            InstanceKind::Module(def_id) => {
                let name = db
                    .module_definitions
                    .get(def_id)
                    .map(|d| d.name.as_str())
                    .unwrap_or("?");
                format!("Mod:{name}")
            }
        };
        format!("{}[{}]#{}", type_name, self.ins, self.index)
    }

    pub fn new(ins: InstanceId, index: u32, kind: PinKind) -> Self {
        Self { ins, index, kind }
    }

    // If this pin is a module pin then it's passing the current into the internal pin.
    // This function returns that internal pin
    // TODO: This is shit. Just make a map of external to internal pins?
    pub fn is_passthrough(&self, db: &DB) -> Option<Self> {
        if matches!(db.circuit.ty(self.ins), InstanceKind::Module(_)) {
            return None;
        }
        let conns = db.circuit.connections_containing(*self);

        let bi_conns: Vec<Connection> = conns
            .iter()
            .filter(|c| c.kind == ConnectionKind::BI)
            .copied()
            .collect();
        if let Some(conn) = bi_conns.into_iter().next() {
            let connected_pin = conn.get_other_pin(*self);
            return Some(connected_pin);
        }
        None
    }
}
