use sheeet_wasm::expression::Expression;
use sheeet_wasm::reference::{CellPointer, usize_to_column_name};
use sheeet_wasm::state::{SerializableState, State, js_value_to_string, log};
use std::cell::RefCell;
use std::collections::HashSet;
use wasm_bindgen::prelude::*;
use web_sys::console::log_2;
use web_sys::window;

#[wasm_bindgen]
pub fn run_evaluate(input: &str) -> JsValue {
    STATE.with_borrow_mut(|state| {
        let expression = Expression::parse(input).unwrap();
        let mut dependencies = HashSet::new();
        match state.resolve_expression_value_and_dependencies(&mut dependencies, &expression) {
            Ok(val) => {
                log_2(&"evaluate ok:".into(), &val);
                val
            }
            Err(err) => {
                log_2(&"evaluate failed:".into(), &err);
                err
            }
        }
    })
}

#[wasm_bindgen]
pub fn get_cell_raw_value(id: &str) -> String {
    let key = CellPointer::from_str(id);
    STATE.with_borrow(|state| state.get_cell_raw_value(key).unwrap_or_default())
}

#[wasm_bindgen]
pub fn set_cell_raw_value(id: &str, raw: &str) -> Result<String, JsValue> {
    STATE.with_borrow_mut(|state| {
        let cell_pointer = CellPointer::from_str(id);
        match &raw.len() {
            // Remove.
            0 => {
                state.remove_cell(cell_pointer)?;
                Ok(String::new())
            }
            // Upsert.
            _ => {
                let resolved_value = state.upsert_cell(cell_pointer, raw)?;
                if resolved_value.is_null() {
                    // Show the raw value if we can't find reference.
                    Ok(state.get_cell_raw_value(cell_pointer).unwrap_or_default())
                } else {
                    Ok(js_value_to_string(resolved_value))
                }
            }
        }
    })
}

#[wasm_bindgen]
pub fn copy_cell_get_raw_value(from_id: &str, to_id: &str) -> Result<String, JsValue> {
    STATE.with_borrow(|state| {
        state.copy_cell_expression(CellPointer::from_str(from_id), CellPointer::from_str(to_id))
    })
}

#[wasm_bindgen]
pub fn save_app_state_to_local_storage() -> Result<(), JsValue> {
    let window = window().ok_or("could not get window")?;
    let local_storage = window
        .local_storage()?
        .ok_or("could not get local storage")?;

    let serialized = STATE.with_borrow(|state| {
        serde_json::to_string(&state.to_serializable_state())
            .map_err(|err| JsValue::from(err.to_string()))
    })?;

    local_storage.set_item("sheet-data", &serialized)?;

    log(&format!("saved app state to local storage:\n{serialized}"));

    Ok(())
}

fn main() {
    console_error_panic_hook::set_once();
    log("log from wasm main");
}

#[wasm_bindgen]
pub fn init_app() -> Result<(), JsValue> {
    if STATE.with_borrow_mut(|state| {
        if state.initialized {
            state.recalculate()?;
        }
        Ok::<bool, JsValue>(state.initialized)
    })? {
        return Ok(());
    }

    let window = window().ok_or("could not get window")?;
    let document = window.document().ok_or("could not get document")?;
    let spreadsheet_table = document
        .get_element_by_id("spreadsheet")
        .ok_or("could not get spreadsheet element")?;

    let local_storage = window
        .local_storage()?
        .ok_or("could not get local storage")?;
    let (columns, rows) = match local_storage.get_item("sheet-data")? {
        Some(data) => {
            let saved_state: SerializableState =
                serde_json::from_str(&data).map_err(|err| JsValue::from(err.to_string()))?;
            let state = saved_state.to_memory_state()?;
            let bounds = state.sheet_bounds;
            STATE.set(state);
            bounds
        }
        None => {
            let state = State::new();
            let bounds = state.sheet_bounds;
            STATE.set(state);
            bounds
        }
    };

    let table_head = document.create_element("thead")?;
    spreadsheet_table.append_with_node_1(&table_head)?;
    let table_body = document.create_element("tbody")?;
    spreadsheet_table.append_with_node_1(&table_body)?;

    STATE.with_borrow(|state| -> Result<(), JsValue> {
        for row in 0..rows {
            match row {
                0 => {
                    for column in 0..columns {
                        let tr = match table_head.first_element_child() {
                            Some(tr_elem) => tr_elem,
                            None => {
                                let tr_elem = document.create_element("tr")?;
                                table_head.append_with_node_1(&tr_elem)?;
                                tr_elem
                            }
                        };
                        let header_val = match column {
                            0 => "",
                            i => &usize_to_column_name(i),
                        };
                        let header_val = header_val.to_uppercase();
                        let th = document.create_element("th")?;
                        th.set_text_content(Some(&header_val));
                        tr.append_with_node_1(&th)?;
                    }
                }
                row => {
                    let tr = document.create_element("tr")?;
                    table_body.append_with_node_1(&tr)?;
                    for column in 0..columns {
                        let td = document.create_element("td")?;
                        let val = match column {
                            0 => Some(row.to_string()),
                            column => {
                                td.set_id(&format!("{}-{}", column, row));
                                let key = CellPointer::from_column_and_row(column, row);
                                match state.get_cell_resolved_value(key) {
                                    Some(value) => Some(js_value_to_string(value)),
                                    None => match state.get_cell_raw_value(key) {
                                        None => None,
                                        Some(val) => Some(format!("unresolved value '{val}'")),
                                    },
                                }
                            }
                        };
                        if let Some(val) = val {
                            td.set_text_content(Some(&val));
                        };
                        tr.append_with_node_1(&td)?;
                    }
                }
            }
        }
        Ok(())
    })?;

    Ok(())
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}
