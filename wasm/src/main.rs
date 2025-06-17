use js_sys::Array;
use sheeet_wasm::{parse_expression, usize_to_column_name, Expression};
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

fn main() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    log("log from wasm main");

    let window = window().ok_or("could not get window")?;
    let document = window.document().ok_or("could not get document")?;
    let spreadsheet_table = document
        .get_element_by_id("spreadsheet")
        .ok_or("could not get spreadsheet element")?;
    // let local_storage = window.local_storage()?;

    let table_head = document.create_element("thead")?;
    spreadsheet_table.append_with_node_1(&table_head)?;
    let table_body = document.create_element("tbody")?;
    spreadsheet_table.append_with_node_1(&table_body)?;

    const COLUMNS: usize = 27;
    const ROWS: usize = 100;
    for row in 0..ROWS {
        match row {
            0 => {
                for column in 0..COLUMNS {
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
                for column in 0..COLUMNS {
                    let val = match column {
                        0 => row.to_string(),
                        column => {
                            format!("{}:{row}", usize_to_column_name(column - 1).to_uppercase())
                        }
                    };
                    let td = document.create_element("td")?;
                    td.set_text_content(Some(&val));
                    td.set_attribute("contenteditable", "true")?;
                    tr.append_with_node_1(&td)?;
                }
            }
        }
    }

    Ok(())
}
