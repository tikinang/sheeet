use super::debug_log;
use crate::expression::Expression;
use crate::reference::{CellPointer, Reference};
use js_sys::Array;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::window;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    pub fn debug(s: &str);

    #[wasm_bindgen(catch, js_namespace = window)]
    pub fn js_evaluate(fn_name: &str, vars: &Array) -> Result<JsValue, JsValue>;
}

#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        if cfg!(feature = "debug-log") {
            debug(&format!($($arg)*));
        }
    };
}

struct Cell {
    parsed_expression: Expression,
    raw_value: String,
    resolved_value: Option<JsValue>,
    resolved_dependencies: Option<HashSet<CellPointer>>,
}

#[derive(Default)]
pub struct State {
    pub initialized: bool,
    pub sheet_bounds: (usize, usize),
    cells: HashMap<CellPointer, Cell>,
    reverse_index: HashMap<CellPointer, HashSet<CellPointer>>,
}

impl State {
    pub fn new() -> Self {
        State {
            initialized: true,
            sheet_bounds: (27, 65),
            cells: HashMap::new(),
            reverse_index: HashMap::new(),
        }
    }

    pub fn to_serializable_state(&self) -> SerializableState {
        let mut serializable_state = SerializableState {
            sheet_bounds: self.sheet_bounds,
            data: HashMap::with_capacity(self.cells.len()),
        };
        for (k, v) in &self.cells {
            serializable_state
                .data
                .insert(k.clone(), v.raw_value.clone());
        }
        serializable_state
    }

    pub fn recalculate(&mut self) -> Result<(), JsValue> {
        for k in self
            .cells
            .keys()
            .map(|k| k.clone())
            .collect::<Vec<CellPointer>>()
        {
            self.resolve_cell_value_and_dependencies(k, ResolveDisplay::Update)?;
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct SerializableState {
    pub sheet_bounds: (usize, usize),
    pub data: HashMap<CellPointer, String>,
}

impl SerializableState {
    pub fn to_memory_state(self) -> Result<State, JsValue> {
        let mut new_state = State {
            initialized: true,
            sheet_bounds: self.sheet_bounds,
            cells: HashMap::with_capacity(self.data.len()),
            reverse_index: HashMap::new(),
        };
        for (k, v) in self.data {
            new_state.insert_cell(k.clone(), &v)?;
        }
        let keys: Vec<CellPointer> = new_state.cells.keys().map(|k| k.clone()).collect();
        for k in keys {
            new_state.resolve_cell_value_and_dependencies(k, ResolveDisplay::Noop)?;
        }
        Ok(new_state)
    }
}

#[derive(Debug, Clone, Copy)]
enum ResolveDisplay {
    Update,
    UpdateNext,
    Noop,
}

impl ResolveDisplay {
    fn next(self) -> Self {
        match self {
            ResolveDisplay::Noop => ResolveDisplay::Noop,
            _ => ResolveDisplay::Update,
        }
    }
}

impl State {
    pub fn get_cell_raw_value(self: &Self, key: CellPointer) -> Option<String> {
        debug_log!("get_cell_raw_value: {key}");
        let cell = self.cells.get(&key)?;
        Some(cell.raw_value.clone())
    }

    pub fn get_cell_resolved_value(self: &Self, key: CellPointer) -> Option<JsValue> {
        debug_log!("get_cell_resolved_value: {key}");
        let cell = self.cells.get(&key)?;
        cell.resolved_value.clone()
    }

    pub fn insert_cell(self: &mut Self, key: CellPointer, raw: &str) -> Result<(), JsValue> {
        debug_log!("insert_cell: {key} -> {raw}");
        let expr = Expression::parse(raw)?;
        let cell = Cell {
            raw_value: raw.to_string(),
            parsed_expression: expr.clone(),
            resolved_value: None,
            resolved_dependencies: None,
        };
        self.cells.insert(key, cell);
        Ok(())
    }

    pub fn upsert_cell(self: &mut Self, key: CellPointer, raw: &str) -> Result<JsValue, JsValue> {
        debug_log!("upsert_cell: {key} -> {raw}");
        let expr = Expression::parse(raw)?;
        self.cells
            .entry(key)
            .and_modify(|cell| {
                cell.raw_value = raw.to_string();
                cell.parsed_expression = expr.clone();
            })
            .or_insert({
                let expr = Expression::parse(raw)?;
                Cell {
                    raw_value: raw.to_string(),
                    parsed_expression: expr.clone(),
                    resolved_value: None,
                    resolved_dependencies: None,
                }
            });
        self.resolve_cell_value_and_dependencies(key, ResolveDisplay::UpdateNext)
    }

    pub fn remove_cell(self: &mut Self, key: CellPointer) -> Result<(), JsValue> {
        debug_log!("remove_cell: {key}");
        if let Some(mut cell) = self.cells.remove(&key) {
            if let Some(dependencies) = cell.resolved_dependencies.take() {
                for dependency in dependencies {
                    self.reverse_index
                        .entry(dependency)
                        .and_modify(|dependents| _ = dependents.remove(&key));
                }
            }
        };
        if let Some(dependents) = self.reverse_index.remove(&key) {
            for dependent in dependents {
                debug_log!("remove_cell: update dependent: {dependent}");
                self.resolve_cell_value_and_dependencies(dependent, ResolveDisplay::Update)?;
            }
        };
        Ok(())
    }

    fn resolve_cell_value_and_dependencies(
        self: &mut Self,
        key: CellPointer,
        display: ResolveDisplay,
    ) -> Result<JsValue, JsValue> {
        debug_log!("resolve_cell_value_and_dependencies: {key} ({display:?})");
        let cell = self.cells.get_mut(&key).unwrap();
        let old_resolved_value = cell.resolved_value.take();
        let old_dependencies = cell.resolved_dependencies.take();
        let parsed_expression = cell.parsed_expression.clone();

        let mut new_dependencies = HashSet::new();
        let new_resolved_value = self
            .resolve_expression_value_and_dependencies(&mut new_dependencies, &parsed_expression)
            .unwrap_or_else(|err| format!("ERROR: {err:?}").into());

        // Update cell's resolved values.
        debug_log!(
            "resolve_cell_value_and_dependencies: update resolved cell value: {key} -> {new_resolved_value:?}"
        );
        if let ResolveDisplay::Update = display {
            display_cell_value(key, new_resolved_value.clone())?;
        }
        self.cells.entry(key).and_modify(|entry| {
            entry.resolved_value = Some(new_resolved_value.clone());
            entry.resolved_dependencies = Some(new_dependencies.clone());
        });

        // Remove old dependencies from the reverse index.
        if let Some(old_dependencies) = &old_dependencies {
            for old_dependency in old_dependencies.difference(&new_dependencies) {
                debug_log!(
                    "resolve_cell_value_and_dependencies: remove from reverse index: {old_dependency} <- [{key}]"
                );
                self.reverse_index
                    .entry(old_dependency.clone())
                    .and_modify(|dependents| _ = dependents.remove(&key));
            }
        }

        // Add new dependencies to the reverse index.
        for new_dependency in new_dependencies.difference(&old_dependencies.unwrap_or_default()) {
            debug_log!(
                "resolve_cell_value_and_dependencies: add to reverse index: {new_dependency} <- [{key}]"
            );
            self.reverse_index
                .entry(new_dependency.clone())
                .and_modify(|dependents| _ = dependents.insert(key))
                .or_insert_with(|| {
                    let mut dependents = HashSet::new();
                    dependents.insert(key);
                    dependents
                });
        }

        // Compare the old and new resolved values, and only if they differ
        // update recursively all dependents.
        if let Some(old_resolved_value) = old_resolved_value {
            if old_resolved_value == new_resolved_value {
                debug_log!("resolve_cell_value_and_dependencies: resolved value is the same");
                return Ok(new_resolved_value);
            }
        }

        if let Some(dependents) = self
            .reverse_index
            .get(&key)
            .map(|dependents| dependents.clone())
        {
            for dependent in dependents {
                debug_log!("resolve_cell_value_and_dependencies: update dependent: {dependent}");
                self.resolve_cell_value_and_dependencies(dependent, display.next())?;
            }
        }

        Ok(new_resolved_value)
    }

    pub fn resolve_expression_value_and_dependencies(
        self: &mut Self,
        dependencies: &mut HashSet<CellPointer>,
        expression: &Expression,
    ) -> Result<JsValue, JsValue> {
        match expression {
            Expression::None => {
                todo!("remove None expression, use option")
            }
            Expression::Function { name, inputs } => {
                let js_inputs = Array::new();
                for input in inputs {
                    let val =
                        self.resolve_expression_value_and_dependencies(dependencies, input)?;
                    js_inputs.push(&val);
                }
                js_evaluate(&name, &js_inputs)
            }
            Expression::Reference(reference) => match reference {
                Reference::Single(key) => {
                    dependencies.insert(key.clone());
                    let target_cell = self.cells.get(key);
                    match target_cell {
                        Some(target_cell) => {
                            let resolved_value = target_cell.resolved_value.clone();
                            let parsed_expression = target_cell.parsed_expression.clone();
                            match resolved_value {
                                Some(value) => Ok(value),
                                None => {
                                    // Here we are not resolved yet. Lazily init.
                                    let mut target_dependencies = HashSet::new();
                                    let target_resolved_value = self
                                        .resolve_expression_value_and_dependencies(
                                            &mut target_dependencies,
                                            &parsed_expression,
                                        )?;
                                    self.cells.entry(key.clone()).and_modify(|entry| {
                                        entry.resolved_value = Some(target_resolved_value.clone());
                                        entry.resolved_dependencies = Some(target_dependencies);
                                    });
                                    Ok(target_resolved_value)
                                }
                            }
                        }
                        None => Err(JsValue::from_str(&format!("reference '{key}' not found"))),
                    }
                }
                Reference::BoundedRange(_, _) => todo!("bounded range"),
                Reference::UnboundedColRange(_, _) => todo!("unbounded col range"),
                Reference::UnboundedRowRange(_, _) => todo!("unbounded row range"),
            },
            Expression::Value(val) => Ok(JsValue::from_str(&val)),
        }
    }
}

fn display_cell_value(key: CellPointer, value: JsValue) -> Result<(), JsValue> {
    let window = window().ok_or("could not get window")?;
    let document = window.document().ok_or("could not get document")?;
    let element = document
        .get_element_by_id(&key.to_string())
        .ok_or(&format!("could not get element by id: {}", key.to_string()))?;
    element.set_text_content(Some(&js_value_to_string(value)));
    Ok(())
}

pub fn js_value_to_string(value: JsValue) -> String {
    if let Some(value) = value.as_string() {
        value
    } else if let Some(value) = value.as_bool() {
        value.to_string()
    } else if let Some(value) = value.as_f64() {
        value.to_string()
    } else {
        format!("unknown JS value type: {:?}", value)
    }
}
