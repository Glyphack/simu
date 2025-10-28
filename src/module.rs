use std::collections::{HashMap, HashSet};

use egui::Pos2;

use crate::{
    app::{App, DB, InstanceId, Pin},
    assets::{PinGraphics, PinKind},
};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ModuleDefinition {
    pub name: String,
    pub pins: Vec<PinKind>,
    pub truth_table: TruthTable,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Default)]
pub struct TruthTable {
    // Map of turned on pins to output turned on pins
    table: HashMap<Vec<Pin>, Vec<Pin>>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Module {
    pub pos: Pos2,
    pub definition_index: usize,
}

impl Module {
    pub fn name(&self) -> String {
        format!("module {}", self.definition_index + 1)
    }

    pub fn display(&self, _db: &DB, id: InstanceId) -> String {
        let mut sb = self.name();
        sb += &format!(" {id}");
        sb
    }
}

impl ModuleDefinition {
    pub fn display_definition(&self) -> String {
        let mut sb = format!("Module: {}\n", self.name);
        sb += &format!("Pins: {}\n", self.pins.len());
        for pin in &self.pins {
            sb += &format!("  {pin}");
        }
        sb += &format!("Truth table entries: {}\n", self.truth_table.table.len());
        for (inputs, outputs) in &self.truth_table.table {
            sb += &format!("  {inputs:?} -> {outputs:?}\n");
        }
        sb
    }
}

impl App {
    pub fn find_free_pins(&self, instances: &HashSet<InstanceId>) -> Vec<Pin> {
        let mut free_pins = Vec::new();

        for &id in instances {
            for pin in self.db.pins_of(id) {
                if !self.is_pin_connected(pin) {
                    free_pins.push(pin);
                }
            }
        }

        free_pins
    }

    pub fn get_pins(&self, pins: Vec<PinGraphics>) -> &'static [PinGraphics] {
        Box::leak(pins.into_boxed_slice())
    }

    pub fn create_custom_circuit(
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

        let mut internal_connections = Vec::new();
        for connection in &self.db.connections {
            if instances.contains(&connection.a.ins) && instances.contains(&connection.b.ins) {
                internal_connections.push(*connection);
            }
        }

        let free_pins = self.find_free_pins(instances);
        if free_pins.is_empty() {
            return Err("Selected components have no free pins to expose".to_owned());
        }
        let truth_table = TruthTable::default();

        let pins = free_pins.iter().map(|p| p.kind).collect();

        let definition = ModuleDefinition {
            name,
            pins,
            truth_table,
        };

        self.db.module_definitions.push(definition);

        Ok(())
    }
}
