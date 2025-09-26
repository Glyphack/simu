use std::collections::HashSet;

use egui::{Pos2, Rect, Vec2};

use crate::{
    app::{App, Connection, InstanceId, InstancePosOffset, Pin},
    assets,
};

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct CustomCircuitDefinition {
    pub name: String,
    pub internal_components: Vec<InstancePosOffset>,
    pub internal_connections: Vec<Connection>,
    pub external_pins: Vec<CustomCircuitPin>,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone, Copy)]
pub struct CustomCircuitPin {
    pub kind: assets::PinKind,
    pub offset: Vec2,
    pub internal_pin: Pin,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct CustomCircuit {
    pub pos: Pos2,
    pub definition_index: usize,
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

    pub fn create_external_pins(&self, _free_pins: &[Pin], _bounds: Rect) -> Vec<CustomCircuitPin> {
        // todo!("Better implementation");
        // let mut external_pins = Vec::new();
        // let gate_size = self.canvas_config.base_gate_size * 2.0;
        //
        // let mut input_pins = Vec::new();
        // let mut output_pins = Vec::new();
        //
        // for &pin in free_pins {
        //     let pin_kind = self.get_pin_kind(pin);
        //     match pin_kind {
        //         assets::PinKind::Input => input_pins.push(pin),
        //         assets::PinKind::Output => output_pins.push(pin),
        //     }
        // }
        //
        // let input_spacing = if input_pins.len() > 1 {
        //     gate_size.y / (input_pins.len() as f32 + 1.0)
        // } else {
        //     0.0
        // };
        //
        // for (i, &pin) in input_pins.iter().enumerate() {
        //     let y_offset = if input_pins.len() == 1 {
        //         0.0
        //     } else {
        //         -gate_size.y / 2.0 + (i + 1) as f32 * input_spacing
        //     };
        //
        //     external_pins.push(CustomCircuitPin {
        //         kind: assets::PinKind::Input,
        //         offset: Vec2::new(-gate_size.x / 2.0, y_offset),
        //         internal_pin: pin,
        //     });
        // }
        //
        // let output_spacing = if output_pins.len() > 1 {
        //     gate_size.y / (output_pins.len() as f32 + 1.0)
        // } else {
        //     0.0
        // };
        //
        // for (i, &pin) in output_pins.iter().enumerate() {
        //     let y_offset = if output_pins.len() == 1 {
        //         0.0
        //     } else {
        //         -gate_size.y / 2.0 + (i + 1) as f32 * output_spacing
        //     };
        //
        //     external_pins.push(CustomCircuitPin {
        //         kind: assets::PinKind::Output,
        //         offset: Vec2::new(gate_size.x / 2.0, y_offset),
        //         internal_pin: pin,
        //     });
        // }
        //
        // external_pins
        Vec::new()
    }

    pub fn create_custom_circuit(
        &mut self,
        name: String,
        instances: &HashSet<InstanceId>,
    ) -> Result<(), String> {
        if instances.is_empty() {
            return Err("No components selected".to_owned());
        }

        let (bounds, internal_components) = self.extract_instances_with_offsets(instances);

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

        let external_pins = self.create_external_pins(&free_pins, bounds);

        let definition = CustomCircuitDefinition {
            name,
            internal_components,
            internal_connections,
            external_pins,
        };

        self.db.custom_circuit_definitions.push(definition);

        Ok(())
    }
}
