use std::collections::HashMap;
use wasm_bindgen::JsValue;

fn add(a: isize, b: isize) -> isize {
    a + b
}

struct Spreadsheet {
    cells_map: HashMap<CellPointer, Cell>,
}

#[derive(Debug)]
struct CellPointer(usize, usize);

struct Cell {
    parents: HashMap<CellPointer, Cell>,
    children: HashMap<CellPointer, Cell>,
    comp_val: Option<JsValue>,
    expr: Option<Expression>,
}

/// =add(A, sub(4, 2))
#[derive(Debug)]
enum Expression {
    None,
    Function {
        name: String,
        inputs: Vec<Expression>,
    },
    Reference(Reference),
    Value(String),
}

const EQUAL_SIGN: char = '=';
const COMMA: char = ',';
const OPENING_BRACKET: char = '(';
const CLOSING_BRACKET: char = ')';

fn parse_expression(mut input: &str) -> Result<Expression, &'static str> {
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

    println!("return value: {taken}");
    // TODO: Determine what taken is.
    Ok(Expression::Value(taken))
}

/// A2, A1:A5, A1:A, A1:1, A
#[derive(Debug)]
enum Reference {
    Single(CellPointer),
    BoundedRange(CellPointer, CellPointer),
    // TODO: Unbounded ranges.
    // UnboundedColRange(CellPointer, usize),
    // UnboundedRowRange(CellPointer, usize),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add() {
        assert_eq!(add(1, 2), 3)
    }

    #[test]
    fn test_parse_expression() {
        let input = "=add(2, sub(4, 2, add(5, 5), 4))";
        let expr = parse_expression(input);
        let expr = expr.expect("parsing failed");
        println!("{expr:#?}");
        match expr {
            Expression::None => {
                panic!("none")
            }
            Expression::Function { name, inputs } => {
                assert_eq!(name, "add");
                assert_eq!(inputs.len(), 2);
            }
            Expression::Reference(_) => {
                panic!("reference")
            }
            Expression::Value(_) => {
                panic!("value")
            }
        }
    }

    #[test]
    fn test_parse_two_commas() {
        let input = "=add(2,, 4)";
        let expr = parse_expression(input);
        let expr = expr.expect_err("parsing ok");
        println!("{expr:#?}");
    }
}
