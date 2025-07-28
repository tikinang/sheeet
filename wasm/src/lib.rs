use js_sys::Array;
use serde::de::{Error, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::{Display, Formatter};
use std::rc::Rc;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;
use web_sys::window;

#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        if cfg!(feature = "debug-log") {
            debug(&format!($($arg)*));
        }
    };
}

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct CellPointer(usize, usize);

impl CellPointer {
    pub fn from_str(s: &str) -> Self {
        let x: Vec<&str> = s.splitn(2, '-').collect();
        CellPointer(
            x[0].parse()
                .expect("failed to parse first part of the cell pointer"),
            x[1].parse()
                .expect("failed to parse second part of the cell pointer"),
        )
    }

    pub fn to_string(&self) -> String {
        self.0.to_string() + "-" + &self.1.to_string()
    }

    pub fn from_column_and_row(column: usize, row: usize) -> Self {
        CellPointer(column, row)
    }
}

impl Display for CellPointer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&usize_to_column_name(self.0))?;
        f.write_str(&self.1.to_string())?;
        Ok(())
    }
}

impl Serialize for CellPointer {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

struct CellPointerVisitor {}

impl<'de> Visitor<'de> for CellPointerVisitor {
    type Value = CellPointer;

    fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
        formatter.write_str("1-1 for pointer (1,1)")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: Error,
    {
        Ok(CellPointer::from_str(v))
    }
}

impl CellPointerVisitor {
    fn new() -> Self {
        CellPointerVisitor {}
    }
}

impl<'de> Deserialize<'de> for CellPointer {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(CellPointerVisitor::new())
    }
}

#[derive(Default)]
pub struct State {
    pub sheet_bounds: (usize, usize),
    cells: HashMap<CellPointer, CellRef>,
    reverse_dependents_index: HashMap<CellPointer, HashSet<CellPointer>>,
}

impl State {
    pub fn new() -> Self {
        State {
            sheet_bounds: (27, 65),
            cells: HashMap::new(),
            reverse_dependents_index: HashMap::new(),
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
                .insert(k.clone(), v.borrow().raw_value.clone());
        }
        serializable_state
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
            sheet_bounds: self.sheet_bounds,
            cells: HashMap::with_capacity(self.data.len()),
            reverse_dependents_index: HashMap::new(),
        };
        for (k, v) in self.data {
            new_state.insert_cell(k.clone(), &v)?;
        }
        let keys: Vec<CellPointer> = new_state.cells.keys().map(|k| k.clone()).collect();
        for k in keys {
            new_state.resolve_cell_value_and_dependencies(k.clone(), true, false)?;
        }
        Ok(new_state)
    }
}

impl State {
    pub fn get_cell_raw_value(self: &Self, key: CellPointer) -> Option<String> {
        debug_log!("get_cell_raw_value: {key}");
        let cell_ref = self.cells.get(&key)?;
        Some(cell_ref.borrow().raw_value.clone())
    }

    pub fn get_cell_resolved_value(self: &Self, key: CellPointer) -> Option<JsValue> {
        debug_log!("get_cell_resolved_value: {key}");
        let cell_ref = self.cells.get(&key)?;
        cell_ref.borrow().resolved_value.clone()
    }

    pub fn insert_cell(self: &mut Self, key: CellPointer, raw: &str) -> Result<(), JsValue> {
        debug_log!("insert_cell: {key} -> {raw}");
        let expr = parse_expression(raw)?;
        let cell = Rc::new(RefCell::new(Cell {
            cell_pointer: key.clone(),
            raw_value: raw.to_string(),
            parsed_expression: expr.clone(),
            resolved_value: None,
            resolved_dependencies: None,
        }));
        self.cells.insert(key, cell);
        Ok(())
    }

    pub fn upsert_cell(self: &mut Self, key: CellPointer, raw: &str) -> Result<JsValue, JsValue> {
        debug_log!("upsert_cell: {key} -> {raw}");
        self.insert_cell(key.clone(), raw)?;
        self.resolve_cell_value_and_dependencies(key, true, true)
    }

    pub fn remove_cell(self: &mut Self, key: CellPointer) -> Result<(), JsValue> {
        debug_log!("remove_cell: {key}");
        if let Some(cell_ref) = self.cells.remove(&key) {
            if let Some(dependencies) = cell_ref.borrow_mut().resolved_dependencies.take() {
                for dependency in dependencies {
                    self.reverse_dependents_index
                        .entry(dependency)
                        .and_modify(|dependents| _ = dependents.remove(&key));
                }
            }
        };
        if let Some(dependents) = self.reverse_dependents_index.remove(&key) {
            for dependent in dependents {
                debug_log!("remove_cell: update dependent: {dependent}");
                self.resolve_cell_value_and_dependencies(dependent, false, true)?;
            }
        };
        Ok(())
    }

    fn resolve_cell_value_and_dependencies(
        self: &mut Self,
        key: CellPointer,
        // TODO: Make typed.
        first_level: bool,
        update_display_value: bool,
    ) -> Result<JsValue, JsValue> {
        debug_log!(
            "resolve_cell_value_and_dependencies: {key} ({first_level}, {update_display_value})"
        );
        let cell_ref = self
            .cells
            .get(&key)
            .map(|cell_ref| cell_ref.clone())
            .unwrap();
        let old_dependencies = cell_ref.borrow_mut().resolved_dependencies.take();

        let mut new_dependencies = Vec::new();
        let resolved_value = self
            .resolve_expression_value_and_dependencies(
                &mut new_dependencies,
                &cell_ref.borrow().parsed_expression,
            )
            .unwrap_or_else(|err| format!("ERROR({err:?})").into());

        // Update cell's resolved values.
        debug_log!(
            "resolve_cell_value_and_dependencies: update resolved cell value: {key} -> {resolved_value:?}"
        );
        if update_display_value && !first_level {
            display_cell_value(key, resolved_value.clone())?;
        }
        cell_ref.borrow_mut().resolved_value = Some(resolved_value.clone());
        cell_ref.borrow_mut().resolved_dependencies = Some(new_dependencies.clone());

        // Remove old dependencies from the reverse index.
        if let Some(old_dependencies) = old_dependencies {
            for old_dependency in old_dependencies {
                debug_log!(
                    "resolve_cell_value_and_dependencies: remove from reverse index: {old_dependency} <- [{key}]"
                );
                self.reverse_dependents_index
                    .entry(old_dependency)
                    .and_modify(|dependents| _ = dependents.remove(&key));
            }
        }

        // Add new dependencies to the reverse index.
        for new_dependency in new_dependencies {
            debug_log!(
                "resolve_cell_value_and_dependencies: add to reverse index: {new_dependency} <- [{key}]"
            );
            self.reverse_dependents_index
                .entry(new_dependency)
                .and_modify(|dependents| _ = dependents.insert(key))
                .or_insert_with(|| {
                    let mut dependents = HashSet::new();
                    dependents.insert(key);
                    dependents
                });
        }

        // TODO: Compare the old and new resolved values,
        //  and only if they differ update the dependents.
        // Update recursively all dependents.
        if let Some(dependents) = self
            .reverse_dependents_index
            .get(&key)
            .map(|dependents| dependents.clone())
        {
            for dependent in dependents {
                debug_log!("resolve_cell_value_and_dependencies: update dependent: {dependent}");
                self.resolve_cell_value_and_dependencies(dependent, false, update_display_value)?;
            }
        }
        Ok(resolved_value)
    }

    pub fn resolve_expression_value_and_dependencies(
        self: &mut Self,
        dependencies: &mut Vec<CellPointer>,
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
                Reference::Single(cell_pointer) => {
                    dependencies.push(cell_pointer.clone());
                    let target_cell_ref = self
                        .cells
                        .get(&cell_pointer)
                        .map(|cell_ref| cell_ref.clone());
                    match target_cell_ref {
                        Some(target_cell_ref) => {
                            let resolved_value = target_cell_ref.borrow().resolved_value.clone();
                            match resolved_value {
                                Some(resolved) => Ok(resolved),
                                None => {
                                    // Here we are not resolved yet. Lazily init.
                                    let mut target_dependencies = Vec::new();
                                    let target_resolved_value = self
                                        .resolve_expression_value_and_dependencies(
                                            &mut target_dependencies,
                                            &target_cell_ref.borrow().parsed_expression,
                                        )?;
                                    target_cell_ref.borrow_mut().resolved_value =
                                        Some(target_resolved_value.clone());
                                    target_cell_ref.borrow_mut().resolved_dependencies =
                                        Some(target_dependencies);
                                    Ok(target_resolved_value)
                                }
                            }
                        }
                        None => Err(JsValue::from_str(&format!(
                            "reference '{cell_pointer}' not found"
                        ))),
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

pub struct Cell {
    pub cell_pointer: CellPointer,
    pub parsed_expression: Expression,
    pub raw_value: String,
    pub resolved_value: Option<JsValue>,
    pub resolved_dependencies: Option<Vec<CellPointer>>,
}

type CellRef = Rc<RefCell<Cell>>;

#[wasm_bindgen]
extern "C" {
    pub fn alert(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    pub fn debug(s: &str);

    #[wasm_bindgen(catch, js_namespace = window)]
    pub fn js_evaluate(fn_name: &str, vars: &Array) -> Result<JsValue, JsValue>;
}

/// =add(A, sub(4, 2))
#[derive(Debug, PartialEq, Clone)]
pub enum Expression {
    None,
    Function {
        name: String,
        inputs: Vec<Expression>,
    },
    Reference(Reference),
    Value(String),
}

const ALPHABET: [char; 26] = [
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z',
];

// TODO: Make column alphabet functions simpler.
// TODO: Change to Result instead of panic.

pub fn column_name_to_usize(name: &str) -> usize {
    let mut index = 0;
    for (multiplier, mut c) in name.chars().enumerate() {
        if !c.is_ascii_alphabetic() {
            panic!("column name has non-ascii-alphabetic char '{c}'")
        }

        c = c.to_ascii_lowercase();
        if multiplier != name.len() - 1 && c != ALPHABET[0] {
            panic!(
                "unexpected ascii-char '{}' at position {} of '{}', only '{}' supported",
                c, multiplier, name, ALPHABET[0]
            )
        }

        // TODO: Different way to find the index?
        let i = ALPHABET
            .binary_search(&c)
            .expect(&format!("column name char '{c}' not found in the alphabet"));
        index = i + (multiplier * ALPHABET.len())
    }
    index + 1
}

pub fn usize_to_column_name(mut index: usize) -> String {
    if index != 0 {
        index -= 1
    }
    let mut name = String::new();
    loop {
        let i = index % ALPHABET.len();
        name.insert(0, ALPHABET[i]);

        let mut has_next = false;
        if index >= ALPHABET.len() {
            index -= ALPHABET.len();
            has_next = true;
        }

        index -= i;
        if index == 0 && !has_next {
            break;
        }
    }
    name
}

/// A2, A1:A5, A1:A, A1:1, AA1:AA5
#[derive(Debug, PartialEq, Clone)]
pub enum Reference {
    Single(CellPointer),
    BoundedRange(CellPointer, CellPointer),
    UnboundedColRange(CellPointer, usize),
    UnboundedRowRange(CellPointer, usize),
}

const COLON: char = ':';

impl Reference {
    pub fn parse(input: &str) -> Result<Self, String> {
        if !input.is_ascii() {
            return Err(format!("input '{input}' is not ascii"));
        }

        let lowercased = input.to_ascii_lowercase();

        let mut taken_alphabetic = String::new();
        let mut taken_numeric = String::new();
        let mut first_part = None;

        for c in lowercased.chars() {
            if taken_alphabetic.len() == 0 && first_part.is_none() && !c.is_ascii_alphabetic() {
                return Err(format!(
                    "not a valid reference, first char '{c}' is not alphabetic"
                ));
            }

            if c == COLON {
                if taken_alphabetic.len() == 0 || (first_part.is_none() && taken_numeric.len() == 0)
                {
                    return Err("not a valid reference, colon too soon".into());
                }

                if first_part.is_some() {
                    return Err("not a valid reference, unexpected extra colon".into());
                }

                first_part = Some(CellPointer(
                    column_name_to_usize(&taken_alphabetic),
                    taken_numeric.parse().expect("not numeric"),
                ));
                taken_alphabetic = String::new();
                taken_numeric = String::new();
                continue;
            }

            if c.is_ascii_alphabetic() {
                if taken_numeric.len() > 0 {
                    return Err("can't take alphabetic, already taken numeric".into());
                }
                taken_alphabetic.push(c);
            } else if c.is_numeric() {
                taken_numeric.push(c);
            } else {
                return Err(format!("invalid character '{c}' as candidate for taken"));
            }
        }

        let first_part = first_part.unwrap_or_else(|| {
            let r = CellPointer(
                column_name_to_usize(&taken_alphabetic),
                taken_numeric.parse().expect("not numeric"),
            );
            taken_alphabetic = String::new();
            taken_numeric = String::new();
            r
        });

        match (taken_alphabetic.len(), taken_numeric.len()) {
            (col, row) if col > 0 && row > 0 => {
                let second_part = CellPointer(
                    column_name_to_usize(&taken_alphabetic),
                    taken_numeric.parse().expect("not numeric"),
                );
                Ok(Reference::BoundedRange(first_part, second_part))
            }
            (col, _) if col > 0 => Ok(Reference::UnboundedColRange(
                first_part,
                column_name_to_usize(&taken_alphabetic),
            )),
            (_, row) if row > 0 => Ok(Reference::UnboundedRowRange(
                first_part,
                taken_numeric.parse().expect("not numeric"),
            )),
            _ => Ok(Reference::Single(first_part)),
        }
    }
}

const EQUAL_SIGN: char = '=';
const COMMA: char = ',';
const OPENING_BRACKET: char = '(';
const CLOSING_BRACKET: char = ')';

// TODO: Support strings.

pub fn parse_expression(mut input: &str) -> Result<Expression, &'static str> {
    println!("--parse expression: '{input}'");
    input = input.strip_prefix(EQUAL_SIGN).unwrap_or_else(|| input);

    let mut taken = String::new();
    let mut function_expr = Expression::None;
    let mut opening_bracket_count: usize = 0;
    for c in input.chars() {
        println!("char: '{c}', bracket_count: {opening_bracket_count}, taken: {taken}");

        if c.is_whitespace() {
            println!("ignoring whitespace");
            continue;
        }

        if c == COMMA {
            if opening_bracket_count == 0 {
                return Err("unexpected comma in expression root, allowed only inside function");
            }
            if opening_bracket_count > 1 {
                taken.push(c);
                continue;
            }
            if taken.len() == 0 {
                return Err("unexpected comma, no arguments between");
            }
            let expr = parse_expression(&taken)?;
            if let Expression::Function { name: _, inputs } = &mut function_expr {
                inputs.push(expr);
            }
            taken = String::new();
            continue;
        }

        if c == OPENING_BRACKET {
            println!("opening bracket");
            opening_bracket_count += 1;
            if opening_bracket_count > 1 {
                taken.push(c);
                continue;
            }
            // state = ParsingState::InsideFunction;
            function_expr = Expression::Function {
                name: taken.clone(),
                inputs: Vec::new(),
            };
            taken = String::new();
            continue;
        }

        if c == CLOSING_BRACKET {
            println!("closing bracket");
            opening_bracket_count -= 1;
            if opening_bracket_count > 0 {
                taken.push(c);
                continue;
            }
            let expr = parse_expression(&taken)?;
            if let Expression::Function { name: _, inputs } = &mut function_expr {
                inputs.push(expr);
            }
            return Ok(function_expr);
        }

        println!("pushing char: {c}");
        taken.push(c);
    }

    if opening_bracket_count > 0 {
        return Err("unclosed function");
    }

    println!("return value: {taken}");
    match Reference::parse(&taken) {
        Ok(reference) => Ok(Expression::Reference(reference)),
        Err(_) => Ok(Expression::Value(taken)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Expression::{Function, Value};

    #[test]
    fn test_parse_expression() {
        {
            let input = "=add(2, sub(4, 2, add(5, 5), 4))";
            let expr = parse_expression(input).expect("parsing failed");
            println!("{expr:#?}");
            assert_eq!(
                expr,
                Function {
                    name: String::from("add"),
                    inputs: vec![
                        Value(String::from("2")),
                        Function {
                            name: String::from("sub"),
                            inputs: vec![
                                Value(String::from("4")),
                                Value(String::from("2")),
                                Function {
                                    name: String::from("add"),
                                    inputs: vec![
                                        Value(String::from("5")),
                                        Value(String::from("5")),
                                    ],
                                },
                                Value(String::from("4")),
                            ],
                        }
                    ],
                }
            );
        }
        {
            let input = "=add(A2, A0:A, 5)";
            let expr = parse_expression(input).expect("parsing failed");
            println!("{expr:#?}");
            assert_eq!(
                expr,
                Function {
                    name: String::from("add"),
                    inputs: vec![
                        Expression::Reference(Reference::Single(CellPointer(1, 2))),
                        Expression::Reference(Reference::UnboundedColRange(CellPointer(1, 0), 1)),
                        Value(String::from("5")),
                    ],
                }
            );
        }
    }

    #[test]
    fn test_parse_expression_two_commas() {
        let input = "=add(2,, 4)";
        let expr = parse_expression(input);
        expr.expect_err("parsing ok");
    }

    #[test]
    fn test_parse_expression_unclosed_bracket() {
        {
            let input = "=add(2, 4";
            let expr = parse_expression(input);
            expr.expect_err("parsing ok");
        }
        {
            let input = "=add(2, 4,";
            let expr = parse_expression(input);
            expr.expect_err("parsing ok");
        }
    }

    #[test]
    fn test_column_name() {
        assert_eq!(column_name_to_usize("A"), 1);
        assert_eq!(column_name_to_usize("a"), 1);
        assert_eq!(column_name_to_usize("Z"), 26);
        assert_eq!(column_name_to_usize("AA"), 27);
        assert_eq!(column_name_to_usize("AAB"), 54);

        assert_eq!(usize_to_column_name(1), "a");
        assert_eq!(usize_to_column_name(26), "z");
        assert_eq!(usize_to_column_name(27), "aa");
        assert_eq!(usize_to_column_name(54), "aab");

        {
            let input = "a";
            assert_eq!(usize_to_column_name(column_name_to_usize(input)), input);
        }
        {
            let input = "b";
            assert_eq!(usize_to_column_name(column_name_to_usize(input)), input);
        }
        {
            let input = "ab";
            assert_eq!(usize_to_column_name(column_name_to_usize(input)), input);
        }
        {
            let input = "aax";
            assert_eq!(usize_to_column_name(column_name_to_usize(input)), input);
        }
    }

    #[test]
    #[should_panic]
    fn test_column_name_panic() {
        column_name_to_usize("abx");
    }

    #[test]
    fn test_parse_reference() {
        // A1, A0, A1:A5, A1:B5, A1:A, A1:1, A100:AB150
        assert_eq!(
            Reference::parse("A1").unwrap(),
            Reference::Single(CellPointer(1, 1))
        );
        assert_eq!(
            Reference::parse("A0").unwrap(),
            Reference::Single(CellPointer(1, 0))
        );
        assert_eq!(
            Reference::parse("A1:A5").unwrap(),
            Reference::BoundedRange(CellPointer(1, 1), CellPointer(1, 5))
        );
        assert_eq!(
            Reference::parse("A1:B5").unwrap(),
            Reference::BoundedRange(CellPointer(1, 1), CellPointer(2, 5))
        );
        assert_eq!(
            Reference::parse("A1:A").unwrap(),
            Reference::UnboundedColRange(CellPointer(1, 1), 1)
        );
        assert_eq!(
            Reference::parse("A1:1").unwrap(),
            Reference::UnboundedRowRange(CellPointer(1, 1), 1)
        );
        assert_eq!(
            Reference::parse("A100:AB150").unwrap(),
            Reference::BoundedRange(CellPointer(1, 100), CellPointer(28, 150))
        );

        Reference::parse("1").expect_err("expected err");
        Reference::parse("1A").expect_err("expected err");
        Reference::parse("A1A").expect_err("expected err");
        Reference::parse("A1:1A").expect_err("expected err");
        Reference::parse("A1::").expect_err("expected err");
        Reference::parse("-").expect_err("expected err");
    }
}
