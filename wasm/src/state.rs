use super::debug_log;
use crate::expression::Expression;
use crate::reference::{CellPointer, Reference};
use js_sys::Array;
use serde::{Deserialize, Serialize};
use std::cmp::{max, min};
use std::collections::{HashMap, HashSet, LinkedList};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;
use web_sys::{CustomEvent, CustomEventInit, window};

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
    resolved_dependencies: Option<Dependencies>,
}

#[derive(Default)]
pub struct State {
    pub initialized: bool,
    pub sheet_bounds: (usize, usize),
    cells: HashMap<CellPointer, Cell>,
    reverse_index_singles: HashMap<CellPointer, HashSet<CellPointer>>,
    reverse_index_cols: HashMap<usize, HashSet<CellPointer>>,
    reverse_index_rows: HashMap<usize, HashSet<CellPointer>>,
}

impl State {
    pub fn new() -> Self {
        State {
            initialized: true,
            sheet_bounds: (27, 65),
            cells: HashMap::new(),
            reverse_index_singles: HashMap::new(),
            reverse_index_cols: HashMap::new(),
            reverse_index_rows: HashMap::new(),
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
            // TODO: Circular dependency check.
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
            reverse_index_singles: HashMap::new(),
            reverse_index_cols: HashMap::new(),
            reverse_index_rows: HashMap::new(),
        };
        for (k, v) in self.data {
            new_state.insert_cell(k.clone(), &v)?;
        }
        let keys: Vec<CellPointer> = new_state.cells.keys().map(|k| k.clone()).collect();
        for key in keys {
            // TODO: Circular dependency check.
            new_state.resolve_cell_value_and_dependencies(key, ResolveDisplay::Noop)?;
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

#[derive(Default, Clone)]
pub struct Dependencies {
    singles: HashSet<CellPointer>,
    cols: HashSet<usize>,
    rows: HashSet<usize>,
}

impl State {
    pub fn get_cell_raw_value(&self, key: CellPointer) -> Option<String> {
        debug_log!("get_cell_raw_value: {key}");
        let cell = self.cells.get(&key)?;
        Some(cell.raw_value.clone())
    }

    pub fn get_cell_resolved_value(&self, key: CellPointer) -> Option<JsValue> {
        debug_log!("get_cell_resolved_value: {key}");
        let cell = self.cells.get(&key)?;
        cell.resolved_value.clone()
    }

    pub fn insert_cell(&mut self, key: CellPointer, raw: &str) -> Result<(), JsValue> {
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

    pub fn copy_cell_expression(
        &self,
        from: CellPointer,
        to: CellPointer,
    ) -> Result<JsValue, JsValue> {
        debug_log!("copy_cell: {from} -> {to}");
        match self.cells.get(&from) {
            None => Err(format!("couldn't copy {from}, cell not found").into()),
            Some(cell) => Ok(JsValue::from_str(
                &cell
                    .parsed_expression
                    .copy_with_distance(from.distance(&to))
                    .to_string(),
            )),
        }
    }

    pub fn upsert_cell(&mut self, key: CellPointer, raw: &str) -> Result<JsValue, JsValue> {
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
        self.check_circular_dependency(key, &expr)?;
        self.resolve_cell_value_and_dependencies(key, ResolveDisplay::UpdateNext)
    }

    pub fn remove_cell(&mut self, key: CellPointer) -> Result<(), JsValue> {
        debug_log!("remove_cell: {key}");
        if let Some(mut cell) = self.cells.remove(&key) {
            if let Some(dependencies) = cell.resolved_dependencies.take() {
                for dependency in dependencies.singles {
                    self.reverse_index_singles
                        .entry(dependency)
                        .and_modify(|dependents| _ = dependents.remove(&key));
                }
                for dependency in dependencies.cols {
                    self.reverse_index_cols
                        .entry(dependency)
                        .and_modify(|dependents| _ = dependents.remove(&key));
                }
                for dependency in dependencies.rows {
                    self.reverse_index_rows
                        .entry(dependency)
                        .and_modify(|dependents| _ = dependents.remove(&key));
                }
            }
        };
        if let Some(dependents) = self.reverse_index_singles.remove(&key) {
            for dependent in dependents {
                debug_log!("remove_cell: update single dependent: {dependent}");
                self.resolve_cell_value_and_dependencies(dependent, ResolveDisplay::Update)?;
            }
        };
        if let Some(dependents) = self.reverse_index_cols.get(&key.0) {
            let dependents = dependents.clone();
            for dependent in &dependents {
                debug_log!("remove_cell: update col dependent: {dependent}");
                self.resolve_cell_value_and_dependencies(
                    dependent.clone(),
                    ResolveDisplay::Update,
                )?;
            }
            if dependents.len() == 0 {
                _ = self.reverse_index_cols.remove(&key.0);
            }
        };
        if let Some(dependents) = self.reverse_index_rows.get(&key.1) {
            let dependents = dependents.clone();
            for dependent in &dependents {
                debug_log!("remove_cell: update row dependent: {dependent}");
                self.resolve_cell_value_and_dependencies(
                    dependent.clone(),
                    ResolveDisplay::Update,
                )?;
            }
            if dependents.len() == 0 {
                _ = self.reverse_index_rows.remove(&key.1);
            }
        };
        Ok(())
    }

    fn check_circular_dependency(
        &self,
        key: CellPointer,
        expression: &Expression,
    ) -> Result<(), JsValue> {
        let mut visited = LinkedList::new();
        visited.push_back(key);
        self.check_circular_dependency_inner(expression, &mut visited)
    }

    fn check_circular_dependency_inner(
        &self,
        expression: &Expression,
        visited: &mut LinkedList<CellPointer>,
    ) -> Result<(), JsValue> {
        match expression {
            Expression::Function { inputs, .. } => {
                for input in inputs {
                    self.check_circular_dependency_inner(input, visited)?;
                }
                Ok(())
            }
            Expression::Reference(reference) => match reference {
                Reference::Single(key) => {
                    self.check_circular_dependency_single_reference(key, visited)
                }
                Reference::BoundedRange(range_start, range_end) => {
                    let min_col = min(range_start.0, range_end.0);
                    let max_col = max(range_start.0, range_end.0);
                    let min_row = min(range_start.1, range_end.1);
                    let max_row = max(range_start.1, range_end.1);
                    for col in min_col..=max_col {
                        for row in min_row..=max_row {
                            let key = CellPointer(col, row);
                            self.check_circular_dependency_single_reference(&key, visited)?;
                        }
                    }
                    Ok(())
                }
                Reference::UnboundedColRange(range_start, col) => {
                    let min_col = range_start.0;
                    let max_col = col.clone();
                    for col in min_col..=max_col {
                        let keys = self
                            .cells
                            .keys()
                            .filter(|key| key.0 == col && key.1 >= range_start.1)
                            .map(|key| key.clone())
                            .collect::<Vec<CellPointer>>();
                        for key in keys {
                            self.check_circular_dependency_single_reference(&key, visited)?;
                        }
                    }
                    Ok(())
                }
                Reference::UnboundedRowRange(range_start, row) => {
                    let min_row = range_start.1;
                    let max_row = row.clone();
                    for col in min_row..=max_row {
                        let keys = self
                            .cells
                            .keys()
                            .filter(|key| key.1 == col && key.0 >= range_start.0)
                            .map(|key| key.clone())
                            .collect::<Vec<CellPointer>>();
                        for key in keys {
                            self.check_circular_dependency_single_reference(&key, visited)?;
                        }
                    }
                    Ok(())
                }
            },
            Expression::Value(_) => Ok(()),
        }
    }

    fn check_circular_dependency_single_reference(
        &self,
        key: &CellPointer,
        visited: &mut LinkedList<CellPointer>,
    ) -> Result<(), JsValue> {
        if let Some(cell) = self.cells.get(key) {
            if visited.contains(key) {
                return Err((&format!(
                    "circular dependency: {key} in chain {:?}",
                    visited
                        .iter()
                        .map(|key| key.to_string())
                        .chain(vec![key.to_string()])
                        .collect::<Vec<String>>()
                ))
                    .into());
            }
            visited.push_back(key.clone());
            self.check_circular_dependency_inner(&cell.parsed_expression, visited)?;
            _ = visited.pop_back();
        }
        Ok(())
    }

    fn resolve_cell_value_and_dependencies(
        &mut self,
        key: CellPointer,
        display: ResolveDisplay,
    ) -> Result<JsValue, JsValue> {
        debug_log!("resolve_cell_value_and_dependencies: {key} ({display:?})");
        let cell = self.cells.get_mut(&key).unwrap();
        let old_resolved_value = cell.resolved_value.take();
        let old_dependencies = cell.resolved_dependencies.take();
        let parsed_expression = cell.parsed_expression.clone();

        let mut new_dependencies = Dependencies::default();
        let new_resolved_value = self
            .resolve_expression_value_and_dependencies(&mut new_dependencies, &parsed_expression)
            .unwrap_or_else(|err| format!("resolve error: {err:?}").into());

        // Update cell's resolved values.
        debug_log!(
            "resolve_cell_value_and_dependencies: update resolved cell value: {key} -> {new_resolved_value:?}"
        );
        if let ResolveDisplay::Update = display {
            dispatch_display_cell_value_event(key, new_resolved_value.clone())?;
        }
        self.cells.entry(key).and_modify(|entry| {
            entry.resolved_value = Some(new_resolved_value.clone());
            entry.resolved_dependencies = Some(new_dependencies.clone());
        });

        // Remove old dependencies from the reverse index.
        if let Some(old_dependencies) = &old_dependencies {
            for old_dependency in old_dependencies
                .singles
                .difference(&new_dependencies.singles)
            {
                debug_log!(
                    "resolve_cell_value_and_dependencies: remove single from reverse index: {old_dependency} <- [{key}]"
                );
                self.reverse_index_singles
                    .entry(old_dependency.clone())
                    .and_modify(|dependents| _ = dependents.remove(&key));
            }
            for old_dependency in old_dependencies.cols.difference(&new_dependencies.cols) {
                debug_log!(
                    "resolve_cell_value_and_dependencies: remove col from reverse index: {old_dependency} <- [{key}]"
                );
                self.reverse_index_cols
                    .entry(old_dependency.clone())
                    .and_modify(|dependents| _ = dependents.remove(&key));
            }
            for old_dependency in old_dependencies.rows.difference(&new_dependencies.rows) {
                debug_log!(
                    "resolve_cell_value_and_dependencies: remove row from reverse index: {old_dependency} <- [{key}]"
                );
                self.reverse_index_rows
                    .entry(old_dependency.clone())
                    .and_modify(|dependents| _ = dependents.remove(&key));
            }
        }

        // Add new dependencies to the reverse index.
        // TODO: Wrap reverse indices functionality to struct and use generics.
        let old_dependencies = old_dependencies.unwrap_or_default();
        for new_dependency in new_dependencies
            .singles
            .difference(&old_dependencies.singles)
        {
            debug_log!(
                "resolve_cell_value_and_dependencies: add single to reverse index: {new_dependency} <- [{key}]"
            );
            self.reverse_index_singles
                .entry(new_dependency.clone())
                .and_modify(|dependents| _ = dependents.insert(key))
                .or_insert_with(|| {
                    let mut dependents = HashSet::new();
                    dependents.insert(key);
                    dependents
                });
        }
        for new_dependency in new_dependencies.cols.difference(&old_dependencies.cols) {
            debug_log!(
                "resolve_cell_value_and_dependencies: add col to reverse index: {new_dependency} <- [{key}]"
            );
            self.reverse_index_cols
                .entry(new_dependency.clone())
                .and_modify(|dependents| _ = dependents.insert(key))
                .or_insert_with(|| {
                    let mut dependents = HashSet::new();
                    dependents.insert(key);
                    dependents
                });
        }
        for new_dependency in new_dependencies.rows.difference(&old_dependencies.rows) {
            debug_log!(
                "resolve_cell_value_and_dependencies: add row to reverse index: {new_dependency} <- [{key}]"
            );
            self.reverse_index_rows
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
            .reverse_index_singles
            .get(&key)
            .map(|dependents| dependents.clone())
        {
            for dependent in dependents {
                debug_log!(
                    "resolve_cell_value_and_dependencies: update single dependent: {dependent}"
                );
                self.resolve_cell_value_and_dependencies(dependent, display.next())?;
            }
        }
        if let Some(dependents) = self
            .reverse_index_cols
            .get(&key.0)
            .map(|dependents| dependents.clone())
        {
            for dependent in dependents {
                debug_log!(
                    "resolve_cell_value_and_dependencies: update col dependent: {dependent}"
                );
                self.resolve_cell_value_and_dependencies(dependent, display.next())?;
            }
        }
        if let Some(dependents) = self
            .reverse_index_rows
            .get(&key.1)
            .map(|dependents| dependents.clone())
        {
            for dependent in dependents {
                debug_log!(
                    "resolve_cell_value_and_dependencies: update row dependent: {dependent}"
                );
                self.resolve_cell_value_and_dependencies(dependent, display.next())?;
            }
        }

        Ok(new_resolved_value)
    }

    pub fn resolve_expression_value_and_dependencies(
        &mut self,
        dependencies: &mut Dependencies,
        expression: &Expression,
    ) -> Result<JsValue, JsValue> {
        match expression {
            Expression::Function { name, inputs } => {
                let js_inputs = Array::new();
                for input in inputs {
                    let val =
                        self.resolve_expression_value_and_dependencies(dependencies, input)?;
                    js_inputs.push(&val);
                }
                debug_log!("call '{name}' with {js_inputs:?}");
                js_evaluate(&name, &js_inputs)
            }
            Expression::Reference(reference) => match reference {
                Reference::Single(key) => {
                    dependencies.singles.insert(key.clone());
                    self.resolve_single_reference_value_and_dependencies(&key)
                }
                Reference::BoundedRange(range_start, range_end) => {
                    let min_col = min(range_start.0, range_end.0);
                    let max_col = max(range_start.0, range_end.0);
                    let min_row = min(range_start.1, range_end.1);
                    let max_row = max(range_start.1, range_end.1);
                    let ref_values = Array::new();
                    for col in min_col..=max_col {
                        for row in min_row..=max_row {
                            let key = CellPointer(col, row);
                            dependencies.singles.insert(key.clone());
                            let ref_value =
                                self.resolve_single_reference_value_and_dependencies(&key)?;
                            if ref_value.is_null() || ref_value.is_undefined() {
                                continue;
                            }
                            ref_values.push(&ref_value);
                        }
                    }
                    Ok(JsValue::from(ref_values))
                }
                Reference::UnboundedColRange(range_start, col) => {
                    let min_col = range_start.0;
                    let max_col = col.clone();
                    let ref_values = Array::new();
                    for col in min_col..=max_col {
                        dependencies.cols.insert(col);
                        let keys = self
                            .cells
                            .keys()
                            .filter(|key| key.0 == col && key.1 >= range_start.1)
                            .map(|key| key.clone())
                            .collect::<Vec<CellPointer>>();
                        for key in keys {
                            let ref_value =
                                self.resolve_single_reference_value_and_dependencies(&key)?;
                            if ref_value.is_null() || ref_value.is_undefined() {
                                continue;
                            }
                            ref_values.push(&ref_value);
                        }
                    }
                    Ok(JsValue::from(ref_values))
                }
                Reference::UnboundedRowRange(range_start, row) => {
                    let min_row = range_start.1;
                    let max_row = row.clone();
                    let ref_values = Array::new();
                    for col in min_row..=max_row {
                        dependencies.rows.insert(col);
                        let keys = self
                            .cells
                            .keys()
                            .filter(|key| key.1 == col && key.0 >= range_start.0)
                            .map(|key| key.clone())
                            .collect::<Vec<CellPointer>>();
                        for key in keys {
                            let ref_value =
                                self.resolve_single_reference_value_and_dependencies(&key)?;
                            if ref_value.is_null() || ref_value.is_undefined() {
                                continue;
                            }
                            ref_values.push(&ref_value);
                        }
                    }
                    Ok(JsValue::from(ref_values))
                }
            },
            Expression::Value(val) => Ok(JsValue::from_str(&val)),
        }
    }

    fn resolve_single_reference_value_and_dependencies(
        &mut self,
        key: &CellPointer,
    ) -> Result<JsValue, JsValue> {
        let target_cell = self.cells.get(key);
        match target_cell {
            Some(target_cell) => {
                let resolved_value = target_cell.resolved_value.clone();
                let parsed_expression = target_cell.parsed_expression.clone();
                match resolved_value {
                    Some(value) => Ok(value),
                    None => {
                        // Here we are not resolved yet. Lazily init.
                        let mut target_dependencies = Dependencies::default();
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
            // None => Err(JsValue::from_str(&format!("reference '{key}' not found"))),
            None => Ok(JsValue::null()), // TODO: Goal is to coerce invalid ref to empty values.
        }
    }
}

pub fn dispatch_display_cell_value_event(key: CellPointer, value: JsValue) -> Result<(), JsValue> {
    let window = window().unwrap();

    let detail = js_sys::Object::new();
    js_sys::Reflect::set(
        &detail,
        &"cellId".into(),
        &JsValue::from_str(&key.to_serializable()),
    )?;
    js_sys::Reflect::set(&detail, &"jsValue".into(), &value)?;

    let event_init = CustomEventInit::new();
    event_init.set_detail(&detail);
    event_init.set_cancelable(true);

    let event = CustomEvent::new_with_event_init_dict("display-cell-value", &event_init)?;
    window.dispatch_event(&event)?;

    Ok(())
}
