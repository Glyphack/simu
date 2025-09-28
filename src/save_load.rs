use std::collections::HashSet;

use slotmap::{Key as _, SecondaryMap, SlotMap};

use crate::{
    App,
    app::{Connection, DB, Gate, InstanceId, InstanceKind, Power, Wire},
    custom_circuit::{CustomCircuit, CustomCircuitDefinition},
};

// Custom serialization format for DB that avoids SlotMap version issues
#[derive(serde::Serialize, serde::Deserialize)]
struct SerializableDB {
    gates: Vec<(u64, Gate)>,
    powers: Vec<(u64, Power)>,
    wires: Vec<(u64, Wire)>,
    custom_circuits: Vec<(u64, CustomCircuit)>,
    custom_circuit_definitions: Vec<CustomCircuitDefinition>,
    connections: HashSet<Connection>,
}

impl serde::Serialize for DB {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut gates = Vec::new();
        let mut powers = Vec::new();
        let mut wires = Vec::new();
        let mut custom_circuits = Vec::new();

        for (id, gate) in &self.gates {
            gates.push((id.data().as_ffi(), *gate));
        }
        for (id, power) in &self.powers {
            powers.push((id.data().as_ffi(), *power));
        }
        for (id, wire) in &self.wires {
            wires.push((id.data().as_ffi(), wire.clone()));
        }
        for (id, custom_circuit) in &self.custom_circuits {
            custom_circuits.push((id.data().as_ffi(), custom_circuit.clone()));
        }

        let serializable = SerializableDB {
            gates,
            powers,
            wires,
            custom_circuits,
            custom_circuit_definitions: self.custom_circuit_definitions.clone(),
            connections: self.connections.clone(),
        };

        serializable.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for DB {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let serializable = SerializableDB::deserialize(deserializer)?;

        let mut db = Self {
            instances: SlotMap::with_key(),
            types: SecondaryMap::new(),
            gates: SecondaryMap::new(),
            powers: SecondaryMap::new(),
            wires: SecondaryMap::new(),
            custom_circuits: SecondaryMap::new(),
            custom_circuit_definitions: serializable.custom_circuit_definitions,
            connections: serializable.connections,
        };

        // Reconstruct gates
        for (raw_id, gate) in serializable.gates {
            let key_data = slotmap::KeyData::from_ffi(raw_id);
            let id = InstanceId::from(key_data);

            // Ensure the slot exists in instances
            while db.instances.len() <= id.data().as_ffi() as usize {
                db.instances.insert(());
            }

            db.gates.insert(id, gate);
            db.types.insert(id, InstanceKind::Gate(gate.kind));
        }

        // Reconstruct powers
        for (raw_id, power) in serializable.powers {
            let key_data = slotmap::KeyData::from_ffi(raw_id);
            let id = InstanceId::from(key_data);

            while db.instances.len() <= id.data().as_ffi() as usize {
                db.instances.insert(());
            }

            db.powers.insert(id, power);
            db.types.insert(id, InstanceKind::Power);
        }

        // Reconstruct wires
        for (raw_id, wire) in serializable.wires {
            let key_data = slotmap::KeyData::from_ffi(raw_id);
            let id = InstanceId::from(key_data);

            while db.instances.len() <= id.data().as_ffi() as usize {
                db.instances.insert(());
            }

            db.wires.insert(id, wire);
            db.types.insert(id, InstanceKind::Wire);
        }

        // Reconstruct custom circuits
        for (raw_id, custom_circuit) in serializable.custom_circuits {
            let key_data = slotmap::KeyData::from_ffi(raw_id);
            let id = InstanceId::from(key_data);

            while db.instances.len() <= id.data().as_ffi() as usize {
                db.instances.insert(());
            }

            let definition_index = custom_circuit.definition_index;
            db.custom_circuits.insert(id, custom_circuit);
            db.types
                .insert(id, InstanceKind::CustomCircuit(definition_index));
        }

        Ok(db)
    }
}

impl App {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_to_file(&self) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;

        let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON files", &["json"])
            .set_file_name("circuit.json")
            .save_file()
        else {
            return Ok(());
        };

        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json)?;
        log::info!("Saved circuit to: {}", path.display());
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn save_to_file(&self) -> Result<(), Box<dyn std::error::Error>> {
        use wasm_bindgen::JsCast;
        use web_sys::{Blob, BlobPropertyBag, Url, window};

        let json = serde_json::to_string_pretty(self)?;

        let Some(window) = window() else {
            return Ok(());
        };
        let Ok(document) = window.document().ok_or("No document") else {
            return Ok(());
        };

        let blob_parts = js_sys::Array::new();
        blob_parts.push(&wasm_bindgen::JsValue::from_str(&json));

        let blob_property_bag = BlobPropertyBag::new();
        blob_property_bag.set_type("application/json");

        let Ok(blob) = Blob::new_with_str_sequence_and_options(&blob_parts, &blob_property_bag)
        else {
            return Ok(());
        };
        let Ok(url) = Url::create_object_url_with_blob(&blob) else {
            return Ok(());
        };
        let Ok(element) = document.create_element("a") else {
            return Ok(());
        };
        let Ok(html_element) = element.dyn_into::<web_sys::HtmlElement>() else {
            return Ok(());
        };

        html_element.set_attribute("href", &url).ok();
        html_element.set_attribute("download", "circuit.json").ok();
        html_element.click();
        Url::revoke_object_url(&url).ok();
        log::info!("Circuit saved as download");
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_from_file(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs;

        let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON files", &["json"])
            .pick_file()
        else {
            return Ok(());
        };

        let json = fs::read_to_string(&path)?;
        let loaded_app: Self = serde_json::from_str(&json)?;
        *self = loaded_app;
        self.current_dirty = true;
        log::info!("Loaded circuit from: {}", path.display());
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load_from_file(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        use wasm_bindgen::JsCast;
        use web_sys::HtmlInputElement;

        let Some(window) = web_sys::window() else {
            return Ok(());
        };
        let Ok(document) = window.document().ok_or("No document") else {
            return Ok(());
        };
        let Ok(element) = document.create_element("input") else {
            return Ok(());
        };
        let Ok(input) = element.dyn_into::<HtmlInputElement>() else {
            return Ok(());
        };

        input.set_type("file");
        input.set_accept("application/json,.json");

        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |event: web_sys::Event| {
            let Some(target) = event.target() else {
                return;
            };
            let Ok(input) = target.dyn_into::<HtmlInputElement>() else {
                return;
            };
            let Some(file_list) = input.files() else {
                return;
            };
            let Some(file) = file_list.get(0) else {
                return;
            };

            let Ok(file_reader) = web_sys::FileReader::new() else {
                return;
            };
            let file_reader_clone = file_reader.clone();

            let onload_closure =
                wasm_bindgen::closure::Closure::wrap(Box::new(move |_event: web_sys::Event| {
                    let Ok(result) = file_reader_clone.result() else {
                        return;
                    };
                    let Some(text) = result.as_string() else {
                        return;
                    };

                    // Store the JSON in localStorage for pickup by the main thread
                    if let Some(win) = web_sys::window() {
                        if let Ok(Some(storage)) = win.local_storage() {
                            if storage.set_item("simu_pending_load", &text).is_ok() {
                                log::info!("File loaded successfully, applying to circuit...");
                            }
                        }
                    }
                }) as Box<dyn FnMut(_)>);

            file_reader.set_onload(Some(onload_closure.as_ref().unchecked_ref()));
            onload_closure.forget();
            file_reader.read_as_text(&file).ok();
        }) as Box<dyn FnMut(_)>);

        input.set_onchange(Some(closure.as_ref().unchecked_ref()));
        closure.forget();
        input.click();

        Ok(())
    }

    pub fn process_pending_load(&mut self) {
        // First check the local field
        if let Some(json) = self.pending_load_json.take() {
            match serde_json::from_str::<Self>(&json) {
                Ok(mut loaded_app) => {
                    loaded_app.pending_load_json = None;
                    *self = loaded_app;
                    self.current_dirty = true;
                    log::info!("Circuit loaded successfully from JSON");
                }
                Err(e) => log::error!("Failed to parse JSON: {e}"),
            }
        } else {
            // Then check localStorage for web version
            #[cfg(target_arch = "wasm32")]
            {
                use web_sys::window;
                if let Some(window) = window() {
                    if let Ok(Some(storage)) = window.local_storage() {
                        if let Ok(Some(json)) = storage.get_item("simu_pending_load") {
                            // Clear the stored JSON immediately to prevent repeated loading
                            storage.remove_item("simu_pending_load").ok();

                            match serde_json::from_str::<Self>(&json) {
                                Ok(mut loaded_app) => {
                                    loaded_app.pending_load_json = None;
                                    *self = loaded_app;
                                    self.current_dirty = true;
                                    log::info!("Circuit loaded successfully from web storage");
                                }
                                Err(e) => log::error!("Failed to parse JSON from web storage: {e}"),
                            }
                        }
                    }
                }
            }
        }
    }
}
