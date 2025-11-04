use std::collections::HashSet;

use egui::Pos2;

use crate::{
    app::App,
    assets::PinGraphics,
    db::{Circuit, DB, InstanceId, ModuleDefId},
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
    pub fn display_definition(&self) -> String {
        let mut sb = format!("Module: {}\n", self.name);
        let circuit_display = self.circuit.display();
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

    /// Returns unconnected pins from the module's circuit.
    /// Returns a Vec of Pins that are not connected to anything within the circuit.
    /// The Pin struct contains the kind field (Input or Output) to distinguish pin types.
    pub fn get_unconnected_pins(&self) -> Vec<crate::db::Pin> {
        use std::collections::HashSet;

        let mut connected_pins = HashSet::new();

        // Collect all pins that are part of a connection
        for conn in &self.circuit.connections {
            connected_pins.insert(conn.a);
            connected_pins.insert(conn.b);
        }

        let mut unconnected_pins = Vec::new();

        // Iterate through all instances and their pins
        for (id, _) in &self.circuit.types {
            let pins = self.circuit.pins_of(id);
            for pin in pins {
                // If pin is not in connected set, it's unconnected
                if !connected_pins.contains(&pin) {
                    unconnected_pins.push(pin);
                }
            }
        }

        unconnected_pins
    }
}

impl App {
    pub fn get_pins(&self, pins: Vec<PinGraphics>) -> &'static [PinGraphics] {
        Box::leak(pins.into_boxed_slice())
    }

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
            internal_components.push(self.db.ty(*instance));
        }

        let mut circuit = Circuit::default();
        let mut id_map = std::collections::HashMap::new();

        for &old_id in instances {
            let new_id = match self.db.ty(old_id) {
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
                    crate::db::Pin::new(new_a_id, conn.a.index, conn.a.kind),
                    crate::db::Pin::new(new_b_id, conn.b.index, conn.b.kind),
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
    fn test_module_creation_with_independent_instance_ids() {
        // Test that module definitions use independent InstanceIds
        // and don't crash when original components are deleted
        let mut app = App::default();

        // Create an AND gate in the main circuit
        let gate = Gate {
            pos: pos2(100.0, 100.0),
            kind: GateKind::And,
        };
        let gate_id = app.db.new_gate(gate);

        // Create a module from the AND gate
        let mut instances = HashSet::new();
        instances.insert(gate_id);
        let result = app.create_module_definition("TestModule".to_owned(), &instances);
        assert!(result.is_ok());

        // Verify the module was created
        assert_eq!(app.db.module_definitions.len(), 1);
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
        assert_eq!(module_def.name, "TestModule");
        assert_eq!(module_def.circuit.types.len(), 1);

        // Delete the AND gate from the main circuit
        app.db.circuit.remove(gate_id);

        // Verify the gate is deleted from main circuit
        assert!(app.db.circuit.gates.get(gate_id).is_none());

        // The module definition should still work and contain its own copy
        let module_def = app
            .db
            .module_definitions
            .get(module_def_id)
            .expect("module def not found after deletion");
        assert_eq!(module_def.circuit.types.len(), 1);

        // Access the module's internal circuit without crashing
        let display = module_def.display_definition();
        assert!(display.contains("TestModule"));
        assert!(display.contains("And"));
    }

    #[test]
    fn test_module_with_multiple_components_and_connections() {
        // Test that connections are properly remapped to new InstanceIds
        let mut app = App::default();

        // Create two gates
        let gate1 = Gate {
            pos: pos2(100.0, 100.0),
            kind: GateKind::And,
        };
        let gate1_id = app.db.new_gate(gate1);

        let gate2 = Gate {
            pos: pos2(200.0, 100.0),
            kind: GateKind::Or,
        };
        let gate2_id = app.db.new_gate(gate2);

        // Create a connection between them
        use crate::connection_manager::Connection;
        use crate::db::Pin;
        let pin1 = Pin::new(gate1_id, 0, crate::assets::PinKind::Output);
        let pin2 = Pin::new(gate2_id, 0, crate::assets::PinKind::Input);
        app.db
            .circuit
            .connections
            .insert(Connection::new(pin1, pin2));

        // Create a module from both gates
        let mut instances = HashSet::new();
        instances.insert(gate1_id);
        instances.insert(gate2_id);
        let result = app.create_module_definition("TwoGateModule".to_owned(), &instances);
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

        // Module should have 2 gates and 1 connection
        assert_eq!(module_def.circuit.types.len(), 2);
        assert_eq!(module_def.circuit.connections.len(), 1);

        // Delete both gates from main circuit
        app.db.circuit.remove(gate1_id);
        app.db.circuit.remove(gate2_id);

        // Module definition should still be accessible
        let module_def = app
            .db
            .module_definitions
            .get(module_def_id)
            .expect("module def not found after deletion");
        assert_eq!(module_def.circuit.types.len(), 2);
        assert_eq!(module_def.circuit.connections.len(), 1);

        // Display should work without crashing
        let display = module_def.display_definition();
        assert!(display.contains("TwoGateModule"));
    }

    #[test]
    fn test_empty_selection_returns_error() {
        // Test that creating a module with no components returns an error
        let mut app = App::default();
        let instances = HashSet::new();
        let result = app.create_module_definition("EmptyModule".to_owned(), &instances);
        assert!(result.is_err());
        assert_eq!(
            result.expect_err("expected error for empty selection"),
            "No components selected"
        );
    }

    #[test]
    fn test_get_unconnected_pins_empty_circuit() {
        // Test that an empty circuit returns no unconnected pins
        let definition = ModuleDefinition {
            name: "Empty".to_owned(),
            circuit: Circuit::default(),
        };

        let pins = definition.get_unconnected_pins();
        assert_eq!(pins.len(), 0);
    }

    #[test]
    fn test_get_unconnected_pins_single_gate_all_unconnected() {
        // Test that a single AND gate with no connections returns all pins as unconnected
        let mut circuit = Circuit::default();
        let gate = Gate {
            pos: pos2(100.0, 100.0),
            kind: GateKind::And,
        };
        let gate_id = circuit.new_gate(gate);

        let definition = ModuleDefinition {
            name: "SingleGate".to_owned(),
            circuit,
        };

        let pins = definition.get_unconnected_pins();

        // AND gate has 2 inputs and 1 output
        assert_eq!(pins.len(), 3);

        let inputs: Vec<_> = pins
            .iter()
            .filter(|p| p.kind == crate::assets::PinKind::Input)
            .collect();
        let outputs: Vec<_> = pins
            .iter()
            .filter(|p| p.kind == crate::assets::PinKind::Output)
            .collect();

        assert_eq!(inputs.len(), 2);
        assert_eq!(outputs.len(), 1);

        // Verify they're from the correct instance
        for pin in &pins {
            assert_eq!(pin.ins, gate_id);
        }
    }

    #[test]
    fn test_get_unconnected_pins_with_internal_connection() {
        // Test that internally connected pins are not returned
        let mut circuit = Circuit::default();

        let gate1 = Gate {
            pos: pos2(100.0, 100.0),
            kind: GateKind::And,
        };
        let gate1_id = circuit.new_gate(gate1);

        let gate2 = Gate {
            pos: pos2(200.0, 100.0),
            kind: GateKind::Or,
        };
        let gate2_id = circuit.new_gate(gate2);

        // Connect gate1 output to gate2 input
        use crate::connection_manager::Connection;
        use crate::db::Pin;
        let pin1 = Pin::new(gate1_id, 1, crate::assets::PinKind::Output); // AND output
        let pin2 = Pin::new(gate2_id, 0, crate::assets::PinKind::Input); // OR input
        circuit.connections.insert(Connection::new(pin1, pin2));

        let definition = ModuleDefinition {
            name: "TwoGates".to_owned(),
            circuit,
        };

        let pins = definition.get_unconnected_pins();

        // gate1 has 2 inputs (unconnected)
        // gate1 output is connected to gate2, so not unconnected
        // gate2 has 2 inputs (1 connected, 1 unconnected)
        // gate2 has 1 output (unconnected)
        // Total: 3 inputs + 1 output = 4 unconnected pins
        assert_eq!(pins.len(), 4);

        let inputs: Vec<_> = pins
            .iter()
            .filter(|p| p.kind == crate::assets::PinKind::Input)
            .collect();
        let outputs: Vec<_> = pins
            .iter()
            .filter(|p| p.kind == crate::assets::PinKind::Output)
            .collect();

        assert_eq!(inputs.len(), 3); // 2 from gate1 + 1 from gate2
        assert_eq!(outputs.len(), 1); // 1 from gate2
    }

    #[test]
    fn test_get_unconnected_pins_with_power_and_lamp() {
        // Test with different component types
        let mut circuit = Circuit::default();

        let power = crate::db::Power {
            pos: pos2(50.0, 100.0),
            on: true,
        };
        let power_id = circuit.new_power(power);

        let lamp = crate::db::Lamp {
            pos: pos2(150.0, 100.0),
        };
        let lamp_id = circuit.new_lamp(lamp);

        // Connect power to lamp
        use crate::connection_manager::Connection;
        use crate::db::Pin;
        let power_pin = Pin::new(power_id, 0, crate::assets::PinKind::Output);
        let lamp_pin = Pin::new(lamp_id, 0, crate::assets::PinKind::Input);
        circuit
            .connections
            .insert(Connection::new(power_pin, lamp_pin));

        let definition = ModuleDefinition {
            name: "PowerLamp".to_owned(),
            circuit,
        };

        let pins = definition.get_unconnected_pins();

        // Both pins are connected, so no unconnected pins
        assert_eq!(pins.len(), 0);
    }

    #[test]
    fn test_get_unconnected_pins_mixed_scenario() {
        // Complex scenario with multiple components and mixed connections
        let mut circuit = Circuit::default();

        let gate1 = Gate {
            pos: pos2(100.0, 100.0),
            kind: GateKind::And,
        };
        let gate1_id = circuit.new_gate(gate1);

        let lamp = crate::db::Lamp {
            pos: pos2(200.0, 100.0),
        };
        let lamp_id = circuit.new_lamp(lamp);

        // Connect gate1 output to lamp (lamp input is connected)
        use crate::connection_manager::Connection;
        use crate::db::Pin;
        let gate_output = Pin::new(gate1_id, 1, crate::assets::PinKind::Output);
        let lamp_input = Pin::new(lamp_id, 0, crate::assets::PinKind::Input);
        circuit
            .connections
            .insert(Connection::new(gate_output, lamp_input));

        let definition = ModuleDefinition {
            name: "Mixed".to_owned(),
            circuit,
        };

        let pins = definition.get_unconnected_pins();

        // gate1 has 2 unconnected inputs
        // gate1 output is connected
        // lamp input is connected
        assert_eq!(pins.len(), 2);

        let inputs: Vec<_> = pins
            .iter()
            .filter(|p| p.kind == crate::assets::PinKind::Input)
            .collect();
        let outputs: Vec<_> = pins
            .iter()
            .filter(|p| p.kind == crate::assets::PinKind::Output)
            .collect();

        assert_eq!(inputs.len(), 2);
        assert_eq!(outputs.len(), 0);
    }
}
