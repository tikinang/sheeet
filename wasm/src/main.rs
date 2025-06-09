use js_sys::Array;
use sheeet_wasm::{parse_expression, Expression};
use wasm_bindgen::prelude::*;
use web_sys::console::log_2;

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

fn main() {
    console_error_panic_hook::set_once();
    log("log from wasm main");
}
