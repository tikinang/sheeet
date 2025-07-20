use js_sys::Array;
use serde::de::{Error, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::rc::Rc;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::JsValue;

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
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
    pub data: HashMap<CellPointer, CellRef>,
}

impl State {
    pub fn new() -> Self {
        State {
            sheet_bounds: (27, 65),
            data: HashMap::new(),
        }
    }

    pub fn to_serializable(&self) -> SerializableState {
        let mut serializable_state = SerializableState {
            sheet_bounds: self.sheet_bounds,
            data: HashMap::with_capacity(self.data.len()),
        };
        for (k, v) in &self.data {
            serializable_state
                .data
                .insert(k.clone(), v.borrow().raw.clone());
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
    pub fn to_state(self) -> Result<State, JsValue> {
        let mut new_state = State {
            sheet_bounds: self.sheet_bounds,
            data: HashMap::with_capacity(self.data.len()),
        };
        for (k, v) in self.data {
            let cell = new_state.new_cell(k.clone(), &v)?;
            new_state.data.insert(k, cell);
        }
        for (_, v) in &new_state.data {
            v.borrow_mut().resolve(&new_state)?;
        }
        Ok(new_state)
    }
}

impl State {
    pub fn new_cell(self: &mut Self, key: CellPointer, raw: &str) -> Result<CellRef, &'static str> {
        let expr = parse_expression(raw)?;
        let cell = Rc::new(RefCell::new(Cell {
            cell_pointer: key.clone(),
            raw: raw.to_string(),
            parsed_expr: expr,
            resolved: None,
            dependencies: HashMap::new(),
            dependents: HashMap::new(),
        }));
        Ok(cell)
    }
}

pub struct Cell {
    pub cell_pointer: CellPointer,
    pub raw: String,
    pub parsed_expr: Expression,
    pub resolved: Option<JsValue>,
    pub dependencies: HashMap<CellPointer, CellRef>,
    pub dependents: HashMap<CellPointer, CellRef>,
}

type CellRef = Rc<RefCell<Cell>>;

pub fn update_cell_dependents(cell_ref: &CellRef, state: &mut State) -> Result<(), JsValue> {
    let cell = cell_ref.borrow_mut();
    for (cell_pointer, dependent) in &cell.dependents {
        log(&format!("update cell dependent: {cell_pointer}"));
        dependent.borrow_mut().resolve(state)?;
        update_cell_dependents(dependent, state)?;
    }
    Ok(())
}

#[wasm_bindgen]
extern "C" {
    pub fn alert(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);

    #[wasm_bindgen(catch, js_namespace = window)]
    pub fn js_evaluate(fn_name: &str, vars: &Array) -> Result<JsValue, JsValue>;
}

//
// impl Serialize for Cell {
//     fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//     where
//         S: Serializer,
//     {
//         serializer.serialize_str(&self.raw)
//     }
// }
//
// impl<'de> DeserializeSeed<'de> for Cell {
//     type Value = Cell;
//
//     fn deserialize<D>(self, deserializer: D) -> Result<Self, D::Error>
//     where
//         D: Deserializer<'de>,
//     {
//         struct CellVisitor<'a> {
//             state: &'a mut AppState,
//         }
//
//         impl<'de, 'a> Visitor<'de> for CellVisitor<'a> {
//             type Value = Cell;
//
//             fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
//                 formatter.write_str("parsable expression")
//             }
//
//             fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
//             where
//                 E: Error,
//             {
//                 Cell::from_str(v).map_err(|err| Error::custom(err))
//             }
//         }
//
//         deserializer.deserialize_str(CellVisitor::new())
//     }
// }

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

impl Cell {
    pub fn resolve(self: &mut Self, state: &State) -> Result<JsValue, JsValue> {
        match &self.resolved {
            Some(resolved) => Ok(resolved.clone()),
            None => {
                let resolved_val = self.resolve_expression(state, None)?;
                self.resolved = Some(resolved_val.clone());
                log(&format!(
                    "set resolved: {} -> {resolved_val:?}",
                    self.cell_pointer
                ));
                Ok(resolved_val)
            }
        }
    }

    pub fn resolve_expression(
        self: &mut Self,
        state: &State,
        expression: Option<Expression>,
    ) -> Result<JsValue, JsValue> {
        let expression = expression.unwrap_or(self.parsed_expr.clone());
        match expression {
            Expression::None => {
                todo!("remove None expression, use option")
            }
            Expression::Function { name, inputs } => {
                let js_inputs = Array::new();
                for input in inputs {
                    let val = self.resolve_expression(state, Some(input))?;
                    js_inputs.push(&val);
                }
                js_evaluate(&name, &js_inputs)
            }
            Expression::Reference(reference) => match reference {
                Reference::Single(cell_pointer) => {
                    log(&format!("resolve single: {cell_pointer}"));
                    let target_cell_ref = state
                        .data
                        .get(&cell_pointer)
                        .map(|cell_ref| cell_ref.clone());
                    match target_cell_ref {
                        Some(target_cell_ref) => {
                            let resolved_value = target_cell_ref.borrow().resolved.clone();
                            log(&format!(
                                "resolve single: {cell_pointer} -> {resolved_value:?}"
                            ));
                            if let Some(self_cell_ref) = state.data.get(&cell_pointer) {
                                log(&format!("insert cell dependent: {cell_pointer}"));
                                target_cell_ref
                                    .borrow_mut()
                                    .dependents
                                    .insert(cell_pointer.clone(), self_cell_ref.clone());
                            };
                            self.dependencies
                                .insert(cell_pointer.clone(), target_cell_ref.clone());
                            match resolved_value {
                                Some(resolved) => Ok(resolved),
                                None => {
                                    // Here we are not resolved yet.
                                    log(&format!("resolve stuff: {cell_pointer}"));
                                    target_cell_ref.borrow_mut().resolve(state)
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
