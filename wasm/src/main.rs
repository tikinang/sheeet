use sheeet_wasm::{
    log, parse_expression, update_cell_dependents, usize_to_column_name, Cell, CellPointer, SerializableState,
    State,
};
use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use web_sys::console::log_2;
use web_sys::window;

#[wasm_bindgen]
pub fn run_evaluate(input: &str) -> JsValue {
    let expr = parse_expression(input).unwrap();
    let mut dummy_cell = Cell {
        cell_pointer: CellPointer::from_column_and_row(0, 0),
        parsed_expr: expr,
        dependents: HashMap::new(),
        dependencies: HashMap::new(),
        resolved: None,
        raw: input.to_string(),
    };
    STATE.with_borrow_mut(|state| match dummy_cell.resolve_expression(state, None) {
        Ok(val) => {
            log_2(&"evaluate ok:".into(), &val);
            val
        }
        Err(err) => {
            log_2(&"evaluate failed:".into(), &err);
            err
        }
    })
}

#[wasm_bindgen]
pub fn get_cell_raw_value(id: &str) -> Result<String, JsValue> {
    let cell = STATE.with_borrow(move |state| {
        state
            .data
            .get(&CellPointer::from_str(id))
            .ok_or_else(|| JsValue::from_str(&format!("expected '{id}' to be found in state")))
            .map(|cell| cell.clone())
    })?;
    Ok(cell.borrow().raw.clone())
}

#[wasm_bindgen]
pub fn set_cell_raw_value(id: &str, raw: &str) -> Result<String, JsValue> {
    STATE.with_borrow_mut(|state| {
        let cell_pointer = CellPointer::from_str(id);
        match &raw.len() {
            0 => {
                // TODO: Update dependencies and dependents.
                state.data.remove(&cell_pointer);
                log(&format!("removed app state entry: {id}"));
                Ok(String::new())
            }
            _ => {
                // TODO:
                //  1. Parse expression.
                //  2. Compute cached value (from new parents).
                //  3. Update both old and new parents' children (delete itself from old and add itself to new).
                //  3. Invalidate and recompute children recursively (children stay the same).
                //  4. Upsert cell.
                let cell_ref = state.data.get(&cell_pointer).map(|cell| cell.clone());
                let resolved_val = match cell_ref {
                    Some(cell_ref) => {
                        log(&format!("update cell: {id} -> '{raw}'"));
                        let mut cell = cell_ref.borrow_mut();
                        cell.parsed_expr = parse_expression(raw)?;
                        cell.raw = raw.to_string();
                        cell.resolved = None;
                        let resolved_value = cell.resolve(state)?;
                        drop(cell);
                        update_cell_dependents(&cell_ref, state)?;
                        resolved_value
                    }
                    None => {
                        log(&format!("insert new cell: {id} -> '{raw}'"));
                        let new_cell = state.new_cell(cell_pointer.clone(), raw)?;
                        let new_cell_clone = new_cell.clone();
                        state.data.insert(cell_pointer, new_cell);
                        new_cell_clone.borrow_mut().resolve(state)?
                    }
                };
                Ok(if let Some(resolved_val) = resolved_val.as_string() {
                    resolved_val
                } else if let Some(resolved_val) = resolved_val.as_bool() {
                    resolved_val.to_string()
                } else if let Some(resolved_val) = resolved_val.as_f64() {
                    resolved_val.to_string()
                } else {
                    format!("unknown JS value type: {:?}", resolved_val)
                })
            }
        }
    })
}

#[wasm_bindgen]
pub fn save_app_state_to_local_storage() -> Result<(), JsValue> {
    let window = window().ok_or("could not get window")?;
    let local_storage = window
        .local_storage()?
        .ok_or("could not get local storage")?;

    let serialized = STATE.with_borrow(|state| {
        serde_json::to_string(&state.to_serializable())
            .map_err(|err| JsValue::from(err.to_string()))
    })?;

    local_storage.set_item("sheet-data", &serialized)?;

    log(&format!("saved app state to local storage:\n{serialized}"));

    Ok(())
}

fn main() {
    console_error_panic_hook::set_once();
    log("log from wasm main");
    // init_app().unwrap();
}

#[wasm_bindgen]
pub fn init_app() -> Result<(), JsValue> {
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
            let state = saved_state.to_state()?;
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
                                state
                                    .data
                                    .get(&CellPointer::from_column_and_row(column, row))
                                    .map(|cell| match &cell.borrow().resolved {
                                        Some(val) => {
                                            if let Some(val) = val.as_string() {
                                                val
                                            } else if let Some(val) = val.as_bool() {
                                                val.to_string()
                                            } else if let Some(val) = val.as_f64() {
                                                val.to_string()
                                            } else {
                                                format!("unknown JS value type: {:?}", val)
                                            }
                                        }
                                        None => format!("unresolved value '{}'", cell.borrow().raw),
                                    })
                            }
                        };
                        match val {
                            None => {}
                            Some(val) => {
                                td.set_text_content(Some(&val));
                            }
                        };
                        // td.set_attribute("contenteditable", "true")?;
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
