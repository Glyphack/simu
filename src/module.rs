use std::collections::HashSet;

use egui::Pos2;

use crate::{
    app::App,
    assets::PinGraphics,
    db::{DB, InstanceId, ModuleDefId, Pin},
};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct ModuleDefinition {
    pub name: String,
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
        let sb = format!("Module: {}\n", self.name);
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
        let definition = ModuleDefinition { name };

        self.db.module_definitions.insert(definition);

        Ok(())
    }
}
