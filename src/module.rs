use std::{
    cmp::Ordering,
    collections::{BTreeMap, HashMap, HashSet},
};

use egui::{Pos2, Vec2};

use crate::{
    app::App,
    assets::PinKind,
    config::CanvasConfig,
    connection_manager::Connection,
    db::{Circuit, DB, InstanceId, InstanceKind, ModuleDefId, Pin},
};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Module {
    pub pos: Pos2,
    pub definition_id: ModuleDefId,
    pub instance_members: Vec<InstanceId>,
    // external pin to internal pin mapping
    pub pins: BTreeMap<Pin, Pin>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ModuleDefinition {
    pub name: String,
    pub circuit: Circuit,
}

impl Module {
    pub fn name(&self, db: &DB) -> String {
        db.get_module_def(self.definition_id).name.clone()
    }

    pub fn definition<'a>(&self, db: &'a DB) -> &'a ModuleDefinition {
        db.get_module_def(self.definition_id)
    }

    pub fn display(&self, db: &DB, id: InstanceId) -> String {
        let mut sb = self.name(db);
        sb += &format!(" {id}");
        sb
    }

    pub(crate) fn pins(&self) -> Vec<Pin> {
        self.pins.keys().copied().collect()
    }
}

impl ModuleDefinition {
    /// Flatten this module definition into a target circuit
    /// Creates copies of all internal instances and marks them as hidden
    /// Returns mapping from external (module) pins to internal (component) pins
    pub fn flatten_into_circuit(
        &self,
        definition_id: ModuleDefId,
        module_id: InstanceId,
        pos: Pos2,
        db: &mut DB,
    ) -> Module {
        let mut def_id_to_world_id: HashMap<InstanceId, InstanceId> = HashMap::new();

        // First, create all instances
        for (definition_id, _) in &self.circuit.types {
            let placed_id = match self.circuit.ty(definition_id) {
                InstanceKind::Gate(kind) => {
                    let gate = *self.circuit.get_gate(definition_id);
                    db.circuit.new_gate(gate)
                }
                InstanceKind::Power => {
                    let power = *self.circuit.get_power(definition_id);
                    db.circuit.new_power(power)
                }
                InstanceKind::Wire => {
                    let wire = *self.circuit.get_wire(definition_id);
                    db.circuit.new_wire(wire)
                }
                InstanceKind::Lamp => {
                    let lamp = *self.circuit.get_lamp(definition_id);
                    db.circuit.new_lamp(lamp)
                }
                InstanceKind::Clock => {
                    let clock = *self.circuit.get_clock(definition_id);
                    db.circuit.new_clock(clock)
                }
                InstanceKind::Module(nested_def_id) => {
                    todo!("Module in module not implemented");
                }
            };

            def_id_to_world_id.insert(definition_id, placed_id);
        }

        // Copy internal connections
        for conn in &self.circuit.connections {
            if let (Some(&new_a_id), Some(&new_b_id)) = (
                def_id_to_world_id.get(&conn.a.ins),
                def_id_to_world_id.get(&conn.b.ins),
            ) {
                let conn = Connection::new(
                    Pin::new(new_a_id, conn.a.index, conn.a.kind),
                    Pin::new(new_b_id, conn.b.index, conn.b.kind),
                );
                db.circuit.connections.insert(conn);
            }
        }

        // Create connections from module pins to internal component pins
        let instances_in_order = self.instances_in_order();
        let mut pins = BTreeMap::new();
        let mut last_pin_index = 0;

        for element_id in instances_in_order {
            let element_world_id = def_id_to_world_id[&element_id];
            let item_pins = self.circuit.pins_of(element_id, db);

            for internal_pin in item_pins {
                if self.circuit.connected_pins(internal_pin).is_empty() {
                    let kind = internal_pin.kind;
                    let external_pin = Pin::new(module_id, last_pin_index, kind);
                    let internal_pin =
                        Pin::new(element_world_id, internal_pin.index, internal_pin.kind);
                    pins.insert(external_pin, internal_pin);
                    let conn = Connection::new_bi(external_pin, internal_pin);
                    db.circuit.connections.insert(conn);
                    last_pin_index += 1;
                }
            }
        }

        let instance_members = def_id_to_world_id.values().copied().collect();
        Module {
            pos,
            definition_id,
            instance_members,
            pins,
        }
    }
    pub fn display_definition(&self, _db: &DB, id: ModuleDefId) -> String {
        // Show only a summary, not the full internal circuit
        format!(
            "Module \"{}\" [{:?}] (internal: {} gates, {} powers, {} lamps, {} conns)",
            self.name,
            id,
            self.circuit.gates.len(),
            self.circuit.powers.len(),
            self.circuit.lamps.len(),
            self.circuit.connections.len(),
        )
    }

    /// Mapping of external pin to internal pin
    pub fn pins_mapping(&self, db: &DB, id: InstanceId) -> HashMap<Pin, Pin> {
        let mut m = HashMap::new();

        for (last_pin_index, pin) in self
            .get_unconnected_internal_pins(db)
            .into_iter()
            .enumerate()
        {
            let mut external_pin = pin;
            external_pin.ins = id;
            external_pin.index = last_pin_index as u32;
            m.insert(external_pin, pin);
        }

        m
    }

    pub fn instances_in_order(&self) -> Vec<InstanceId> {
        let mut instances: Vec<InstanceId> = self.circuit.types.keys().collect();
        instances.sort_by(|s, o| {
            let self_id = *s;
            let self_pos = match self.circuit.ty(self_id) {
                InstanceKind::Gate(_) => self.circuit.get_gate(self_id).pos,
                InstanceKind::Power => self.circuit.get_power(self_id).pos,
                InstanceKind::Wire => self.circuit.get_wire(self_id).center(),
                InstanceKind::Lamp => self.circuit.get_lamp(self_id).pos,
                InstanceKind::Clock => self.circuit.get_clock(self_id).pos,
                InstanceKind::Module(module_def_id) => self.circuit.get_module(self_id).pos,
            };

            let other_id = *o;
            let other_pos = match self.circuit.ty(other_id) {
                InstanceKind::Gate(_) => self.circuit.get_gate(other_id).pos,
                InstanceKind::Power => self.circuit.get_power(other_id).pos,
                InstanceKind::Wire => self.circuit.get_wire(other_id).center(),
                InstanceKind::Lamp => self.circuit.get_lamp(other_id).pos,
                InstanceKind::Clock => self.circuit.get_clock(other_id).pos,
                InstanceKind::Module(_) => self.circuit.get_module(other_id).pos,
            };

            if self_pos.y > other_pos.y {
                Ordering::Greater
            } else if self_pos.y == other_pos.y {
                if self_pos.x > other_pos.x {
                    Ordering::Greater
                } else if self_pos.x == other_pos.x {
                    Ordering::Equal
                } else {
                    Ordering::Less
                }
            } else {
                Ordering::Less
            }
        });
        instances
    }

    pub fn get_unconnected_internal_pins(&self, db: &DB) -> Vec<Pin> {
        let mut unconnected_pins = Vec::new();

        for id in self.instances_in_order() {
            let pins = self.circuit.pins_of(id, db);
            for pin in pins {
                if self.circuit.connected_pins(pin).is_empty() {
                    unconnected_pins.push(pin);
                }
            }
        }

        unconnected_pins
    }

    /// Calculates the offset of a pin from the module's center position.
    /// This matches the layout logic used in rendering (app.rs `draw_module`).
    /// Input pins are placed on the left side, outputs on the right.
    /// Multiple pins of the same kind are evenly spaced vertically.
    pub fn calculate_pin_offset(
        &self,
        db: &DB,
        pins: &[Pin],
        pin: &Pin,
        canvas_config: &CanvasConfig,
    ) -> Vec2 {
        let mut input_indices = vec![];
        let mut output_indices = vec![];
        for (i, pin) in pins.iter().enumerate() {
            match pin.kind {
                PinKind::Input => input_indices.push(i),
                PinKind::Output => output_indices.push(i),
            }
        }

        let base_size = canvas_config.base_gate_size;
        let left_x = -base_size.x / 2.0;
        let right_x = base_size.x / 2.0;
        let top_y = -base_size.y / 2.0;

        let pin_index_usize = pin.index as usize;

        let pin = &pins[pin_index_usize];
        let (x, local_index, indices) = match pin.kind {
            PinKind::Input => (
                left_x,
                input_indices
                    .iter()
                    .position(|&i| i == pin_index_usize)
                    .expect("pin must exist"),
                &input_indices,
            ),
            PinKind::Output => (
                right_x,
                output_indices
                    .iter()
                    .position(|&i| i == pin_index_usize)
                    .expect("pin must exist"),
                &output_indices,
            ),
        };

        let num = indices.len();
        let y = if num == 1 {
            0.0
        } else {
            let spacing = base_size.y / (num - 1) as f32;
            top_y + local_index as f32 * spacing
        };

        Vec2::new(x, y)
    }
}

impl App {
    pub fn create_module_definition(
        &mut self,
        name: String,
        instances: &HashSet<InstanceId>,
    ) -> Result<(), String> {
        if instances.is_empty() {
            return Err("No components selected".to_owned());
        }

        let mut circuit = Circuit::default();
        let mut id_map = std::collections::HashMap::new();

        for &old_id in instances {
            let new_id = match self.db.circuit.ty(old_id) {
                crate::db::InstanceKind::Gate(kind) => {
                    let gate = *self.db.circuit.get_gate(old_id);
                    circuit.new_gate(gate)
                }
                crate::db::InstanceKind::Power => {
                    let power = *self.db.circuit.get_power(old_id);
                    circuit.new_power(power)
                }
                crate::db::InstanceKind::Wire => {
                    let wire = *self.db.circuit.get_wire(old_id);
                    circuit.new_wire(wire)
                }
                crate::db::InstanceKind::Lamp => {
                    let lamp = *self.db.circuit.get_lamp(old_id);
                    circuit.new_lamp(lamp)
                }
                crate::db::InstanceKind::Clock => {
                    let clock = *self.db.circuit.get_clock(old_id);
                    circuit.new_clock(clock)
                }
                crate::db::InstanceKind::Module(def_id) => {
                    todo!("Cloning modules is not yet supported");
                }
            };
            id_map.insert(old_id, new_id);
        }

        for conn in &self.db.circuit.connections {
            if let (Some(&new_a_id), Some(&new_b_id)) =
                (id_map.get(&conn.a.ins), id_map.get(&conn.b.ins))
            {
                let new_conn = crate::connection_manager::Connection::new(
                    Pin::new(new_a_id, conn.a.index, conn.a.kind),
                    Pin::new(new_b_id, conn.b.index, conn.b.kind),
                );
                circuit.connections.insert(new_conn);
            }
        }

        let definition = ModuleDefinition { name, circuit };

        self.db.module_definitions.insert(definition);

        Ok(())
    }
}
