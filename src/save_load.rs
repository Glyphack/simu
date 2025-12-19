use crate::App;

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
