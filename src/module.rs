use std::collections::HashSet;

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

    /// Returns unconnected pins from the module's circuit.
    /// Returns a Vec of Pins that are not connected to anything within the circuit.
    /// The Pin struct contains the kind field (Input or Output) to distinguish pin types.
    pub fn get_unconnected_pins(&self, db: &DB, module_id: InstanceId) -> Vec<crate::db::Pin> {
        use std::collections::HashSet;

        let mut connected_pins = HashSet::new();

        for conn in &self.circuit.connections {
            connected_pins.insert(conn.a);
            connected_pins.insert(conn.b);
        }

        let mut unconnected_pins = Vec::new();

        for (id, _) in &self.circuit.types {
            let pins = self.circuit.pins_of(id, db);
            for mut pin in pins {
                if !connected_pins.contains(&pin) {
                    pin.ins = module_id;
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
        if pin_index_usize >= pins.len() {
            return Vec2::ZERO;
        }

        let pin = &pins[pin_index_usize];
        let (x, local_index, indices) = match pin.kind {
            PinKind::Input => (
                left_x,
                input_indices
                    .iter()
                    .position(|&i| i == pin_index_usize)
                    .unwrap_or(0),
                &input_indices,
            ),
            PinKind::Output => (
                right_x,
                output_indices
                    .iter()
                    .position(|&i| i == pin_index_usize)
                    .unwrap_or(0),
                &output_indices,
            ),
        };

        let num = indices.len();
        let y = if num == 1 {
            0.0 // Centered
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
}
