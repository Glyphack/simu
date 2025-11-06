use std::collections::{HashMap, HashSet};

use egui::{Pos2, Vec2};

use crate::{
    app::App,
    assets::PinKind,
    config::CanvasConfig,
    db::{Circuit, DB, InstanceId, ModuleDefId, Pin},
};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ModuleDefinition {
    pub name: String,
    pub circuit: Circuit,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Module {
    pub pos: Pos2,
    pub definition_index: ModuleDefId,
}

impl Module {
    pub fn name(&self, db: &DB) -> String {
        db.get_module_def(self.definition_index).name.clone()
    }

    pub fn definition<'a>(&self, db: &'a DB) -> &'a ModuleDefinition {
        db.get_module_def(self.definition_index)
    }

    pub fn display(&self, db: &DB, id: InstanceId) -> String {
        let mut sb = self.name(db);
        sb += &format!(" {id}");
        sb
    }
}

impl ModuleDefinition {
    pub fn display_definition(&self, db: &DB, id: ModuleDefId) -> String {
        let mut sb = format!("Module({:?}) {}\n", id, self.name);
        let circuit_display = self.circuit.display(db);
        for line in circuit_display.lines() {
            if line.is_empty() {
                sb.push('\n');
            } else {
                sb.push_str("  ");
                sb.push_str(line);
                sb.push('\n');
            }
        }
        sb
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

    pub fn get_unconnected_internal_pins(&self, db: &DB) -> Vec<Pin> {
        let mut unconnected_pins = Vec::new();

        for (id, _) in &self.circuit.types {
            let pins = self.circuit.pins_of(id, db);
            for pin in pins {
                if self.circuit.connected_pins(pin).is_empty() {
                    unconnected_pins.push(pin);
                }
            }
        }

        unconnected_pins
    }

    pub fn get_unconnected_pins(&self, db: &DB, module_id: InstanceId) -> Vec<Pin> {
        let mut unconnected_pins = Vec::new();
        for (last_pin_index, mut pin) in self
            .get_unconnected_internal_pins(db)
            .into_iter()
            .enumerate()
        {
            pin.ins = module_id;
            pin.index = last_pin_index as u32;
            unconnected_pins.push(pin);
        }
        unconnected_pins
    }

    /// Calculates the offset of a pin from the module's center position.
    /// This matches the layout logic used in rendering (app.rs `draw_module`).
    /// Input pins are placed on the left side, outputs on the right.
    /// Multiple pins of the same kind are evenly spaced vertically.
    pub fn calculate_pin_offset(&self, db: &DB, pin: &Pin, canvas_config: &CanvasConfig) -> Vec2 {
        let pins = self.get_unconnected_pins(db, pin.ins);

        // Separate input and output indices
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

    /// Flatten this module definition into a target circuit
    /// Creates copies of all internal instances and marks them as hidden
    /// Returns mapping from external (module) pins to internal (component) pins
    pub fn flatten_into_circuit(
        &self,
        target_circuit: &mut Circuit,
        module_id: InstanceId,
        definition_id: ModuleDefId,
        db: &DB,
    ) -> HashMap<Pin, Pin> {
        // Map old instance IDs to new ones
        let mut id_map: HashMap<InstanceId, InstanceId> = HashMap::new();

        // Copy all instances from definition circuit to target circuit
        for (old_id, _) in &self.circuit.types {
            let new_id = match self.circuit.ty(old_id) {
                crate::db::InstanceKind::Gate(kind) => {
                    let gate = *self.circuit.get_gate(old_id);
                    target_circuit.new_gate(gate)
                }
                crate::db::InstanceKind::Power => {
                    let power = *self.circuit.get_power(old_id);
                    target_circuit.new_power(power)
                }
                crate::db::InstanceKind::Wire => {
                    let wire = *self.circuit.get_wire(old_id);
                    target_circuit.new_wire(wire)
                }
                crate::db::InstanceKind::Lamp => {
                    let lamp = *self.circuit.get_lamp(old_id);
                    target_circuit.new_lamp(lamp)
                }
                crate::db::InstanceKind::Clock => {
                    let clock = *self.circuit.get_clock(old_id);
                    target_circuit.new_clock(clock)
                }
                crate::db::InstanceKind::Module(nested_def_id) => {
                    // Handle nested modules recursively
                    let nested_module = self.circuit.get_module(old_id).clone();
                    let nested_id = target_circuit.new_module(nested_module);

                    // Recursively flatten the nested module
                    let nested_def = db.get_module_def(nested_def_id);
                    let _nested_pin_map = nested_def.flatten_into_circuit(
                        target_circuit,
                        nested_id,
                        nested_def_id,
                        db,
                    );

                    nested_id
                }
            };

            // Mark the new instance as hidden
            target_circuit.mark_as_hidden(new_id, module_id, definition_id);
            id_map.insert(old_id, new_id);
        }

        // Copy all internal connections with remapped IDs
        for conn in &self.circuit.connections {
            if let (Some(&new_a_id), Some(&new_b_id)) =
                (id_map.get(&conn.a.ins), id_map.get(&conn.b.ins))
            {
                let new_conn = crate::connection_manager::Connection::new(
                    Pin::new(new_a_id, conn.a.index, conn.a.kind),
                    Pin::new(new_b_id, conn.b.index, conn.b.kind),
                );
                target_circuit.connections.insert(new_conn);
            }
        }

        // Build pin mapping: external pins -> internal pins
        let mut pin_mapping = HashMap::new();
        for (external_pin_index, internal_pin) in self
            .get_unconnected_internal_pins(db)
            .into_iter()
            .enumerate()
        {
            // External pin (on the module boundary)
            let external_pin = Pin::new(module_id, external_pin_index as u32, internal_pin.kind);

            // Internal pin (on the actual component, with remapped instance ID)
            let new_internal_id = *id_map
                .get(&internal_pin.ins)
                .expect("internal pin instance should be in id_map");
            let remapped_internal_pin =
                Pin::new(new_internal_id, internal_pin.index, internal_pin.kind);

            pin_mapping.insert(external_pin, remapped_internal_pin);
        }

        pin_mapping
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

        let mut internal_components = Vec::new();
        for instance in instances {
            internal_components.push(self.db.circuit.ty(*instance));
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
                    let module = self.db.circuit.get_module(old_id).clone();
                    circuit.new_module(module)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Gate, GateKind};
    use egui::pos2;

    #[test]
    fn test_panic_lol() {
        let mut app = App::default();

        let gate = Gate {
            pos: pos2(100.0, 100.0),
            kind: GateKind::And,
        };
        let gate_id = app.db.circuit.new_gate(gate);

        let mut instances = HashSet::new();
        instances.insert(gate_id);
        let result = app.create_module_definition("module".to_owned(), &instances);
        assert!(result.is_ok());

        let module_def_id = app
            .db
            .module_definitions
            .keys()
            .next()
            .expect("module def id not found");
        let module_def = app
            .db
            .module_definitions
            .get(module_def_id)
            .expect("module def not found");

        let module_id = app.db.circuit.new_module(Module {
            pos: Pos2::ZERO,
            definition_index: module_def_id,
        });
        app.db.circuit.remove(gate_id);

        assert!(app.db.circuit.gates.get(gate_id).is_none());

        let pins = app.db.circuit.pins_of(module_id, &app.db);

        for pin in pins {
            eprintln!("getting pin {}", pin.display(&app.db.circuit));
            app.db
                .circuit
                .pin_position(pin, &CanvasConfig::default(), &app.db);
        }

        let display = module_def.display_definition(&app.db, module_def_id);
    }

    #[test]
    fn test_simple_module_flattening() {
        // Create a module definition with a single AND gate
        let mut definition_circuit = Circuit::default();
        let gate = crate::db::Gate {
            pos: pos2(100.0, 100.0),
            kind: crate::db::GateKind::And,
        };
        let gate_id = definition_circuit.new_gate(gate);

        let module_def = ModuleDefinition {
            name: "SimpleAND".to_owned(),
            circuit: definition_circuit,
        };

        // Create DB and add definition
        let mut db = DB::default();
        let def_id = db.module_definitions.insert(module_def);

        // Create a module instance in target circuit
        let mut target_circuit = Circuit::default();
        let module = Module {
            pos: Pos2::ZERO,
            definition_index: def_id,
        };
        let module_id = target_circuit.new_module(module);

        // Flatten the module
        let module_def = db.get_module_def(def_id);
        let pin_map = module_def.flatten_into_circuit(&mut target_circuit, module_id, def_id, &db);

        // Check that internal gate was created and marked as hidden
        let hidden_instances = target_circuit.get_hidden_instances_for_module(module_id);
        assert_eq!(hidden_instances.len(), 1, "Should have 1 hidden instance");

        // Should have 2 total instances (module + hidden gate)
        assert_eq!(target_circuit.types.len(), 2);

        // Should have 1 visible instance (just the module)
        assert_eq!(target_circuit.visible_instances().count(), 1);

        // Pin mapping should have 3 entries (2 inputs + 1 output for AND gate)
        assert_eq!(pin_map.len(), 3, "AND gate has 3 unconnected pins");
    }

    #[test]
    fn test_module_flattening_with_connections() {
        // Create a module definition with two gates connected
        let mut definition_circuit = Circuit::default();

        let gate1 = crate::db::Gate {
            pos: pos2(50.0, 100.0),
            kind: crate::db::GateKind::And,
        };
        let gate1_id = definition_circuit.new_gate(gate1);

        let gate2 = crate::db::Gate {
            pos: pos2(150.0, 100.0),
            kind: crate::db::GateKind::Or,
        };
        let gate2_id = definition_circuit.new_gate(gate2);

        // Connect output of gate1 to input of gate2
        let gate1_output = Pin::new(gate1_id, 1, PinKind::Output);
        let gate2_input = Pin::new(gate2_id, 0, PinKind::Input);
        definition_circuit
            .connections
            .insert(crate::connection_manager::Connection::new(
                gate1_output,
                gate2_input,
            ));

        let module_def = ModuleDefinition {
            name: "TwoGates".to_owned(),
            circuit: definition_circuit,
        };

        // Create DB and add definition
        let mut db = DB::default();
        let def_id = db.module_definitions.insert(module_def);

        // Create a module instance in target circuit
        let mut target_circuit = Circuit::default();
        let module = Module {
            pos: Pos2::ZERO,
            definition_index: def_id,
        };
        let module_id = target_circuit.new_module(module);

        // Flatten the module
        let module_def = db.get_module_def(def_id);
        let _pin_map = module_def.flatten_into_circuit(&mut target_circuit, module_id, def_id, &db);

        // Should have 2 hidden instances (both gates)
        let hidden_instances = target_circuit.get_hidden_instances_for_module(module_id);
        assert_eq!(hidden_instances.len(), 2);

        // Should have 3 total instances (module + 2 gates)
        assert_eq!(target_circuit.types.len(), 3);

        // The internal connection should be copied
        assert_eq!(
            target_circuit.connections.len(),
            1,
            "Internal connection should be copied"
        );
    }

    #[test]
    fn test_nested_module_flattening() {
        // Create an inner module definition (just an AND gate)
        let mut inner_circuit = Circuit::default();
        let inner_gate = crate::db::Gate {
            pos: pos2(50.0, 50.0),
            kind: crate::db::GateKind::And,
        };
        let _inner_gate_id = inner_circuit.new_gate(inner_gate);

        let inner_def = ModuleDefinition {
            name: "InnerModule".to_owned(),
            circuit: inner_circuit,
        };

        // Create DB and add inner definition
        let mut db = DB::default();
        let inner_def_id = db.module_definitions.insert(inner_def);

        // Create an outer module definition containing the inner module
        let mut outer_circuit = Circuit::default();
        let inner_module_instance = Module {
            pos: pos2(100.0, 100.0),
            definition_index: inner_def_id,
        };
        let _inner_module_id = outer_circuit.new_module(inner_module_instance);

        let outer_def = ModuleDefinition {
            name: "OuterModule".to_owned(),
            circuit: outer_circuit,
        };

        let outer_def_id = db.module_definitions.insert(outer_def);

        // Create an instance of the outer module in target circuit
        let mut target_circuit = Circuit::default();
        let outer_module = Module {
            pos: Pos2::ZERO,
            definition_index: outer_def_id,
        };
        let outer_module_id = target_circuit.new_module(outer_module);

        // Flatten the outer module (which should recursively flatten the inner module)
        let outer_def = db.get_module_def(outer_def_id);
        let _pin_map =
            outer_def.flatten_into_circuit(&mut target_circuit, outer_module_id, outer_def_id, &db);

        // Check what's hidden to the outer module (should be just the inner module)
        let outer_hidden = target_circuit.get_hidden_instances_for_module(outer_module_id);
        assert_eq!(
            outer_hidden.len(),
            1,
            "Outer module should have 1 direct child (inner module)"
        );

        // The inner module should be in the outer's hidden instances
        let inner_module_id = outer_hidden[0];
        assert!(matches!(
            target_circuit.ty(inner_module_id),
            crate::db::InstanceKind::Module(_)
        ));

        // Check what's hidden to the inner module (should be the inner gate)
        let inner_hidden = target_circuit.get_hidden_instances_for_module(inner_module_id);
        assert_eq!(
            inner_hidden.len(),
            1,
            "Inner module should have 1 hidden instance (the gate)"
        );

        // Total instances: outer module + inner module (hidden) + inner gate (hidden to inner)
        assert_eq!(target_circuit.types.len(), 3);

        // Only 1 instance should be visible (the outer module)
        assert_eq!(target_circuit.visible_instances().count(), 1);
    }
}
