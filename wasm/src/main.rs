use js_sys::Array;
use serde::{Deserialize, Serialize};
use sheeet_wasm::{parse_expression, usize_to_column_name, CellPointer, Expression};
use std::cell::RefCell;
use std::collections::hash_map::Entry::{Occupied, Vacant};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use web_sys::console::log_2;
use web_sys::window;

#[wasm_bindgen]
extern "C" {
    fn alert(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);

    #[wasm_bindgen(catch, js_namespace = window)]
    fn evaluate(fn_name: &str, vars: &Array) -> Result<JsValue, JsValue>;
}

#[wasm_bindgen]
pub fn run_evaluate(input: &str) -> JsValue {
    let expression = parse_expression(input).unwrap();
    match iterate_expression(expression) {
        Ok(val) => {
            log_2(&"evaluate ok:".into(), &val);
            val
        }
        Err(err) => {
            log_2(&"evaluate failed:".into(), &err);
            err
        }
    }
}

#[wasm_bindgen]
pub fn update_app_state(k: &str, v: &str) {
    STATE.with_borrow_mut(|state| {
        let cell_pointer = CellPointer::from_str(k);
        match state.data.entry(cell_pointer) {
            Occupied(mut entry) => {
                entry.insert(v.to_string());
            }
            Vacant(entry) => {
                entry.insert(v.to_string());
            }
        };
        log(&format!("updated state app: {k} -> {v}"));
    });
}

#[wasm_bindgen]
pub fn save_app_state_to_local_storage() -> Result<(), JsValue> {
    let window = window().ok_or("could not get window")?;
    let local_storage = window
        .local_storage()?
        .ok_or("could not get local storage")?;

    let serialized = STATE.with_borrow(|state| {
        serde_json::to_string(state).map_err(|err| JsValue::from(err.to_string()))
    })?;

    local_storage.set_item("sheet-data", &serialized)?;

    log(&format!("saved app state to local storage:\n{serialized}"));

    Ok(())
}

fn iterate_expression(expression: Expression) -> Result<JsValue, JsValue> {
    match expression {
        Expression::None => {
            todo!("remove None expression, use option")
        }
        Expression::Function { name, inputs } => {
            let js_inputs = Array::new();
            for input in inputs {
                let val = iterate_expression(input)?;
                js_inputs.push(&val);
            }
            evaluate(&name, &js_inputs)
        }
        Expression::Reference(reference) => {
            todo!("references")
        }
        Expression::Value(val) => Ok(JsValue::from_str(&val)),
    }
}

#[derive(Default, Serialize, Deserialize)]
struct AppState {
    sheet_bound_columns: usize,
    sheet_bound_rows: usize,
    data: HashMap<CellPointer, String>,
}

thread_local! {
    static STATE: RefCell<AppState> = RefCell::new(AppState::default());
}

fn main() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    log("log from wasm main");

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
            let saved_state: AppState =
                serde_json::from_str(&data).map_err(|err| JsValue::from(err.to_string()))?;
            let dimensions = (
                saved_state.sheet_bound_columns,
                saved_state.sheet_bound_rows,
            );
            STATE.with_borrow_mut(move |state| {
                state.sheet_bound_rows = saved_state.sheet_bound_rows;
                state.sheet_bound_columns = saved_state.sheet_bound_columns;
                state.data = saved_state.data;
            });
            dimensions
        }
        None => {
            let dimensions = STATE.with_borrow_mut(|state| {
                state.sheet_bound_columns = 27;
                state.sheet_bound_rows = 65;
                state.data = HashMap::new();
                (state.sheet_bound_columns, state.sheet_bound_rows)
            });
            dimensions
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
                            i => &usize_to_column_name(i - 1),
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
                        let val = match column {
                            0 => Some(row.to_string()),
                            column => state
                                .data
                                .get(&CellPointer::from_column_and_row(column, row))
                                .map(|x| x.clone()),
                        };
                        let td = document.create_element("td")?;
                        match val {
                            None => {}
                            Some(val) => {
                                td.set_text_content(Some(&val));
                            }
                        };
                        td.set_attribute("contenteditable", "true")?;
                        td.set_id(&format!("{column}-{row}"));
                        tr.append_with_node_1(&td)?;
                    }
                }
            }
        }
        Ok(())
    })?;

    Ok(())
}
