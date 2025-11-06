use std::collections::{HashMap, HashSet};
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

/// Metadata for instances that are hidden from UI (flattened module internals)
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy)]
pub struct HiddenMetadata {
    /// The module instance that owns this hidden instance
    pub parent_module: InstanceId,
    /// The module definition ID (for handling definition updates)
    pub definition_id: ModuleDefId,
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
    // Hidden instances (module internals) - not rendered in UI but evaluated in simulation
    pub hidden_instances: SecondaryMap<InstanceId, HiddenMetadata>,
    // Pin mappings for modules: external pin -> internal pin
    #[serde(skip)]
    pub module_pin_mappings: SecondaryMap<InstanceId, HashMap<Pin, Pin>>,
}
impl Circuit {
    pub fn ty(&self, id: InstanceId) -> InstanceKind {
        self.types
            .get(id)
            .copied()
            .unwrap_or_else(|| panic!("instance type missing for id: {id:?}"))
    }

    pub fn remove(&mut self, id: InstanceId) {
        // If this is a module, cascade delete all hidden instances
        if matches!(self.ty(id), InstanceKind::Module(_)) {
            let hidden_children = self.get_hidden_instances_for_module(id);
            for child_id in hidden_children {
                self.remove_single_instance(child_id);
            }
        }

        self.remove_single_instance(id);
    }

    /// Remove a single instance without cascade deletion
    fn remove_single_instance(&mut self, id: InstanceId) {
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
        self.hidden_instances.remove(id);
        self.types.remove(id);
        self.connections.retain(|c| !c.involves_instance(id));
    }

    /// Check if an instance is hidden from UI
    pub fn is_hidden(&self, id: InstanceId) -> bool {
        self.hidden_instances.contains_key(id)
    }

    /// Get all instances that are visible in UI (not hidden)
    pub fn visible_instances(&self) -> impl Iterator<Item = InstanceId> + '_ {
        self.types.keys().filter(|id| !self.is_hidden(*id))
    }

    /// Get all hidden instances that belong to a specific module
    pub fn get_hidden_instances_for_module(&self, module_id: InstanceId) -> Vec<InstanceId> {
        self.hidden_instances
            .iter()
            .filter_map(|(id, metadata)| {
                if metadata.parent_module == module_id {
                    Some(id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Mark an instance as hidden (internal to a module)
    pub fn mark_as_hidden(
        &mut self,
        id: InstanceId,
        parent_module: InstanceId,
        definition_id: ModuleDefId,
    ) {
        self.hidden_instances.insert(
            id,
            HiddenMetadata {
                parent_module,
                definition_id,
            },
        );
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

    pub fn new_module(&mut self, c: crate::module::Module) -> InstanceId {
        let k = self.types.insert(InstanceKind::Module(c.definition_index));
        self.modules.insert(k, c);
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

    pub fn display(&self, db: &DB) -> String {
        let mut out = String::new();
        use std::fmt::Write as _;
        writeln!(out, "circuit.types:").ok();
        for (id, _) in &self.types {
            writeln!(out, "  {}: {:?}", id, self.ty(id)).ok();
        }
        writeln!(
            out,
            "\ncounts: gates={}, powers={}, lamps={}, clocks={}, wires={}, modules={}, conns={}",
            self.gates.len(),
            self.powers.len(),
            self.lamps.len(),
            self.clocks.len(),
            self.wires.len(),
            self.modules.len(),
            self.connections.len(),
        )
        .ok();

        if !self.gates.is_empty() {
            writeln!(out, "\nGates:").ok();
            for (id, g) in &self.gates {
                writeln!(out, "  {}", g.display(id)).ok();
                for (i, pin) in g.kind.graphics().pins.iter().enumerate() {
                    let pin_offset = pin.offset;
                    let p = g.pos + pin_offset;
                    let pin_instance = Pin::new(id, i as u32, pin.kind);
                    writeln!(
                        out,
                        "    {} at ({:.1},{:.1})",
                        pin_instance.display(self),
                        p.x,
                        p.y
                    )
                    .ok();
                }
            }
        }

        if !self.powers.is_empty() {
            writeln!(out, "\nPowers:").ok();
            for (id, p) in &self.powers {
                writeln!(out, "  {}", p.display(id)).ok();
                for (i, pin) in p.graphics().pins.iter().enumerate() {
                    let pin_offset = pin.offset;
                    let pp = p.pos + pin_offset;
                    let pin_instance = Pin::new(id, i as u32, pin.kind);
                    writeln!(
                        out,
                        "    {} at ({:.1},{:.1})",
                        pin_instance.display(self),
                        pp.x,
                        pp.y
                    )
                    .ok();
                }
            }
        }

        if !self.lamps.is_empty() {
            writeln!(out, "\nLamps:").ok();
            for (id, lamp) in &self.lamps {
                writeln!(out, "  {}", lamp.display(id)).ok();
                for (i, pin) in lamp.graphics().pins.iter().enumerate() {
                    let pin_offset = pin.offset;
                    let p = lamp.pos + pin_offset;
                    let pin_instance = Pin::new(id, i as u32, pin.kind);
                    writeln!(
                        out,
                        "    {} at ({:.1},{:.1})",
                        pin_instance.display(self),
                        p.x,
                        p.y
                    )
                    .ok();
                }
            }
        }

        if !self.clocks.is_empty() {
            writeln!(out, "\nClocks:").ok();
            for (id, clock) in &self.clocks {
                writeln!(out, "  {}", clock.display(id)).ok();
                for (i, pin) in clock.graphics().pins.iter().enumerate() {
                    let pin_offset = pin.offset;
                    let p = clock.pos + pin_offset;
                    let pin_instance = Pin::new(id, i as u32, pin.kind);
                    writeln!(
                        out,
                        "    {} at ({:.1},{:.1})",
                        pin_instance.display(self),
                        p.x,
                        p.y
                    )
                    .ok();
                }
            }
        }

        if !self.wires.is_empty() {
            writeln!(out, "\nWires:").ok();
            for (id, w) in &self.wires {
                writeln!(out, "  {}", w.display(id)).ok();
                for pin in self.pins_of(id, db) {
                    writeln!(out, "    {}", pin.display_alone()).ok();
                }
            }
        }

        if !self.modules.is_empty() {
            writeln!(out, "\nModules:").ok();
            for (id, m) in &self.modules {
                writeln!(out, "  {id} (module instance)").ok();
            }
        }

        if !self.connections.is_empty() {
            writeln!(out, "\nConnections:").ok();
            for c in &self.connections {
                writeln!(out, "  {}", c.display(self)).ok();
            }
        }

        out
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
            InstanceKind::Module(def_id) => {
                let module_def = db.get_module_def(def_id);
                module_def.get_unconnected_pins(db, id)
            }
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
                module_def.calculate_pin_offset(db, &pin, canvas_config)
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

    /// Create a new module instance and automatically flatten its internal components
    /// Creates connections between module boundary pins and internal component pins
    ///
    /// Note: This requires temporarily cloning the definition to avoid borrow checker issues.
    /// In the future, we could optimize this with better data structure design.
    pub fn new_module_with_flattening(&mut self, module: crate::module::Module) -> InstanceId {
        let definition_id = module.definition_index;

        // Create the module instance first
        let module_id = self.circuit.new_module(module);

        // Clone the definition to avoid borrow conflicts
        let definition = self.get_module_def(definition_id).clone();

        // Clone module_definitions to avoid borrow conflict
        let module_defs = self.module_definitions.clone();

        // Create a temporary DB with only the definitions we need
        let temp_db = Self {
            circuit: Circuit::default(), // Not used
            module_definitions: module_defs,
        };

        // Flatten it and store the pin mapping
        let pin_mapping =
            definition.flatten_into_circuit(&mut self.circuit, module_id, definition_id, &temp_db);

        // Store the pin mapping
        self.circuit
            .module_pin_mappings
            .insert(module_id, pin_mapping);

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

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;

    #[test]
    fn test_hidden_instance_tracking() {
        let mut circuit = Circuit::default();

        // Create a gate
        let gate = Gate {
            pos: Pos2::ZERO,
            kind: GateKind::And,
        };
        let gate_id = circuit.new_gate(gate);

        // Initially, gate should be visible
        assert!(!circuit.is_hidden(gate_id));
        assert_eq!(circuit.visible_instances().count(), 1);

        // Mark gate as hidden (pretend it belongs to a module)
        let fake_module_id = InstanceId::from(999);
        let fake_def_id = ModuleDefId::from(1);
        circuit.mark_as_hidden(gate_id, fake_module_id, fake_def_id);

        // Now gate should be hidden
        assert!(circuit.is_hidden(gate_id));
        assert_eq!(circuit.visible_instances().count(), 0);

        // Hidden metadata should be accessible
        let hidden = circuit.get_hidden_instances_for_module(fake_module_id);
        assert_eq!(hidden.len(), 1);
        assert_eq!(hidden[0], gate_id);
    }

    #[test]
    fn test_cascade_deletion() {
        let mut circuit = Circuit::default();

        // Create a module instance
        let module = Module {
            pos: Pos2::ZERO,
            definition_index: ModuleDefId::from(1),
        };
        let module_id = circuit.new_module(module);

        // Create some "internal" gates and mark them as hidden
        let gate1 = Gate {
            pos: Pos2::ZERO,
            kind: GateKind::And,
        };
        let gate1_id = circuit.new_gate(gate1);
        circuit.mark_as_hidden(gate1_id, module_id, ModuleDefId::from(1));

        let gate2 = Gate {
            pos: Pos2::ZERO,
            kind: GateKind::Or,
        };
        let gate2_id = circuit.new_gate(gate2);
        circuit.mark_as_hidden(gate2_id, module_id, ModuleDefId::from(1));

        // Should have 3 total instances (1 module + 2 gates)
        assert_eq!(circuit.types.len(), 3);
        // But only 1 visible (the module)
        assert_eq!(circuit.visible_instances().count(), 1);

        // Remove the module - should cascade delete hidden instances
        circuit.remove(module_id);

        // All instances should be gone
        assert_eq!(circuit.types.len(), 0);
        assert_eq!(circuit.visible_instances().count(), 0);
    }

    #[test]
    fn test_visible_instances_filter() {
        let mut circuit = Circuit::default();

        // Create mix of visible and hidden instances
        let gate1 = Gate {
            pos: Pos2::ZERO,
            kind: GateKind::And,
        };
        let gate1_id = circuit.new_gate(gate1);

        let gate2 = Gate {
            pos: Pos2::ZERO,
            kind: GateKind::Or,
        };
        let gate2_id = circuit.new_gate(gate2);

        let power = Power {
            pos: Pos2::ZERO,
            on: true,
        };
        let power_id = circuit.new_power(power);

        // Mark gate2 as hidden
        let fake_module_id = InstanceId::from(999);
        circuit.mark_as_hidden(gate2_id, fake_module_id, ModuleDefId::from(1));

        // Should have 3 total, 2 visible
        assert_eq!(circuit.types.len(), 3);
        let visible: Vec<_> = circuit.visible_instances().collect();
        assert_eq!(visible.len(), 2);
        assert!(visible.contains(&gate1_id));
        assert!(visible.contains(&power_id));
        assert!(!visible.contains(&gate2_id));
    }

    #[test]
    fn test_get_hidden_instances_for_module() {
        let mut circuit = Circuit::default();

        // Create two modules
        let module1 = Module {
            pos: Pos2::ZERO,
            definition_index: ModuleDefId::from(1),
        };
        let module1_id = circuit.new_module(module1);

        let module2 = Module {
            pos: Pos2::ZERO,
            definition_index: ModuleDefId::from(2),
        };
        let module2_id = circuit.new_module(module2);

        // Create gates belonging to different modules
        let gate1 = Gate {
            pos: Pos2::ZERO,
            kind: GateKind::And,
        };
        let gate1_id = circuit.new_gate(gate1);
        circuit.mark_as_hidden(gate1_id, module1_id, ModuleDefId::from(1));

        let gate2 = Gate {
            pos: Pos2::ZERO,
            kind: GateKind::Or,
        };
        let gate2_id = circuit.new_gate(gate2);
        circuit.mark_as_hidden(gate2_id, module1_id, ModuleDefId::from(1));

        let gate3 = Gate {
            pos: Pos2::ZERO,
            kind: GateKind::Xor,
        };
        let gate3_id = circuit.new_gate(gate3);
        circuit.mark_as_hidden(gate3_id, module2_id, ModuleDefId::from(2));

        // Check each module's hidden instances
        let module1_hidden = circuit.get_hidden_instances_for_module(module1_id);
        assert_eq!(module1_hidden.len(), 2);
        assert!(module1_hidden.contains(&gate1_id));
        assert!(module1_hidden.contains(&gate2_id));

        let module2_hidden = circuit.get_hidden_instances_for_module(module2_id);
        assert_eq!(module2_hidden.len(), 1);
        assert!(module2_hidden.contains(&gate3_id));
    }

    #[test]
    fn test_new_module_with_automatic_flattening() {
        // Create a module definition
        let mut definition_circuit = Circuit::default();
        let gate = Gate {
            pos: Pos2::ZERO,
            kind: GateKind::And,
        };
        let _gate_id = definition_circuit.new_gate(gate);

        let module_def = crate::module::ModuleDefinition {
            name: "TestModule".to_owned(),
            circuit: definition_circuit,
        };

        // Create DB and add definition
        let mut db = DB::default();
        let def_id = db.module_definitions.insert(module_def);

        // Create module using new_module_with_flattening
        let module = crate::module::Module {
            pos: Pos2::ZERO,
            definition_index: def_id,
        };
        let module_id = db.new_module_with_flattening(module);

        // Check that flattening happened automatically
        let hidden_instances = db.circuit.get_hidden_instances_for_module(module_id);
        assert_eq!(
            hidden_instances.len(),
            1,
            "Should have auto-flattened 1 instance"
        );

        // Pin mapping should be stored
        assert!(db.circuit.module_pin_mappings.contains_key(module_id));
        let pin_map = db
            .circuit
            .module_pin_mappings
            .get(module_id)
            .expect("module pin mappings should exist");
        assert_eq!(pin_map.len(), 3, "AND gate has 3 pins");
    }
}
