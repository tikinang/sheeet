use crate::reference::{COLON, Reference, usize_to_column_name};
use std::fmt::{Display, Formatter, Write};

#[macro_export]
macro_rules! test_log {
    ($($arg:tt)*) => {
        #[cfg(test)]
        println!($($arg)*);
    };
}

/// =add(A, sub(4, 2))
#[derive(Debug, PartialEq, Clone)]
pub enum Expression {
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
const DOUBLE_QUOTE: char = '"';

impl Expression {
    pub fn parse(input: &str) -> Result<Expression, &'static str> {
        Self::parse_inner(input, true)
    }

    fn parse_inner(mut input: &str, root: bool) -> Result<Expression, &'static str> {
        test_log!("--parse expression: '{input}'");
        input = match input.strip_prefix(EQUAL_SIGN) {
            Some(input) => input,
            None => {
                if root {
                    return Ok(Expression::Value(input.to_string()));
                }
                input
            }
        };

        let mut taken = String::new();
        let mut quoted: Option<String> = None;
        let mut function_expr: Option<Expression> = None;
        let mut opening_bracket_count: usize = 0;
        for c in input.chars() {
            test_log!(
                r#"char: '{c}' | bracket_count: {opening_bracket_count} | taken: "{taken}" | quoted: {quoted:?}"#
            );

            if c == DOUBLE_QUOTE {
                match quoted.take() {
                    Some(quoted) => {
                        test_log!(r#"ending quoted: "{quoted}""#);
                        if let Some(Expression::Function { inputs, .. }) = &mut function_expr {
                            inputs.push(Expression::Value(quoted));
                        }
                    }
                    None => {
                        test_log!("new quoted");
                        quoted = Some(String::new());
                    }
                };
                continue;
            }

            if let Some(quoted) = &mut quoted {
                test_log!("pushing char to quoted: {c}");
                quoted.push(c);
                continue;
            }

            if c == COMMA {
                if opening_bracket_count == 0 {
                    return Err(
                        "unexpected comma in expression root, allowed only inside function",
                    );
                }
                if opening_bracket_count > 1 {
                    taken.push(c);
                    continue;
                }
                if taken.len() == 0 {
                    return Err("unexpected comma, no arguments between");
                }
                if taken.trim().len() > 0 {
                    let expr = Self::parse_inner(&taken, false)?;
                    if let Some(Expression::Function { inputs, .. }) = &mut function_expr {
                        inputs.push(expr);
                    }
                }
                taken = String::new();
                continue;
            }

            if c == OPENING_BRACKET {
                test_log!("opening bracket");
                opening_bracket_count += 1;
                if opening_bracket_count > 1 {
                    taken.push(c);
                    continue;
                }
                function_expr = Some(Expression::Function {
                    name: taken.trim().to_string(),
                    inputs: Vec::new(),
                });
                taken = String::new();
                continue;
            }

            if c == CLOSING_BRACKET {
                test_log!("closing bracket");
                opening_bracket_count -= 1;
                if opening_bracket_count > 0 {
                    taken.push(c);
                    continue;
                }
                if taken.trim().len() > 0 {
                    let expr = Self::parse_inner(&taken.trim(), false)?;
                    if let Some(Expression::Function { inputs, .. }) = &mut function_expr {
                        inputs.push(expr);
                    }
                }
                return function_expr.ok_or("expected function expression to be present");
            }

            test_log!("pushing char: {c}");
            taken.push(c);
        }

        if opening_bracket_count > 0 {
            return Err("unclosed function");
        }

        test_log!(r#"return value: "{taken}""#);
        match Reference::parse(&taken.trim()) {
            Ok(reference) => Ok(Expression::Reference(reference)),
            Err(_) => Ok(Expression::Value(taken.trim().to_string())),
        }
    }

    pub fn copy_with_distance(&self, distance: (isize, isize)) -> Self {
        match self {
            Expression::Function { inputs, name } => {
                let mut new_inputs = Vec::with_capacity(inputs.len());
                for input in inputs.clone() {
                    new_inputs.push(input.copy_with_distance(distance))
                }
                Expression::Function {
                    inputs: new_inputs,
                    name: name.clone(),
                }
            }
            Expression::Reference(reference) => match reference {
                Reference::Single(cell_pointer) => {
                    Expression::Reference(Reference::Single(cell_pointer.add(distance)))
                }
                Reference::BoundedRange(range_start, range_end) => Expression::Reference(
                    Reference::BoundedRange(range_start.add(distance), range_end.add(distance)),
                ),
                Reference::UnboundedColRange(range_start, col) => {
                    Expression::Reference(Reference::UnboundedColRange(
                        range_start.add(distance),
                        col.checked_add_signed(distance.0).unwrap(),
                    ))
                }
                Reference::UnboundedRowRange(range_start, row) => {
                    Expression::Reference(Reference::UnboundedRowRange(
                        range_start.add(distance),
                        row.checked_add_signed(distance.1).unwrap(),
                    ))
                }
            },
            Expression::Value(value) => {
                if let Ok(mut parsed_val) = value.parse::<isize>() {
                    parsed_val = parsed_val + distance.1;
                    Expression::Value(parsed_val.to_string())
                } else {
                    Expression::Value(value.clone())
                }
            }
        }
    }
}

impl Display for Expression {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Expression::Function { name, inputs } => {
                f.write_char(EQUAL_SIGN)?;
                f.write_str(name)?;
                f.write_char(OPENING_BRACKET)?;
                for (i, input) in inputs.iter().enumerate() {
                    input.fmt(f)?;
                    if i < inputs.len() - 1 {
                        f.write_char(COMMA)?;
                    }
                }
                f.write_char(CLOSING_BRACKET)?;
            }
            Expression::Reference(reference) => match reference {
                Reference::Single(key) => {
                    f.write_str(&key.to_string())?;
                }
                Reference::BoundedRange(range_start, range_end) => {
                    f.write_str(&range_start.to_string())?;
                    f.write_char(COLON)?;
                    f.write_str(&range_end.to_string())?;
                }
                Reference::UnboundedColRange(range_start, col) => {
                    f.write_str(&range_start.to_string())?;
                    f.write_char(COLON)?;
                    // TODO: Look around and maybe remove some clone().
                    f.write_str(&usize_to_column_name(col.clone()))?;
                }
                Reference::UnboundedRowRange(range_start, row) => {
                    f.write_str(&range_start.to_string())?;
                    f.write_char(COLON)?;
                    f.write_str(&row.to_string())?;
                }
            },
            Expression::Value(value) => {
                f.write_str(&value)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::expression::Expression::{Function, Value};
    use crate::reference::CellPointer;

    #[test]
    fn test_parse_expression() {
        {
            let input = "=add(2, sub(4, 2, add(5, 5), 4))";
            let expr = Expression::parse(input).expect("parsing failed");
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
            let expr = Expression::parse(input).expect("parsing failed");
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
        {
            let input = "2";
            let expr = Expression::parse(input).expect("parsing failed");
            println!("{expr:#?}");
            assert_eq!(expr, Value(String::from("2")));
        }
        {
            let input = "some text";
            let expr = Expression::parse(input).expect("parsing failed");
            println!("{expr:#?}");
            assert_eq!(expr, Value(String::from("some text")));
        }
        {
            let input = r#"=concat(A1:A, ", ")"#;
            let expr = Expression::parse(input).expect("parsing failed");
            println!("{expr:#?}");
            assert_eq!(
                expr,
                Function {
                    name: String::from("concat"),
                    inputs: vec![
                        Expression::Reference(Reference::UnboundedColRange(CellPointer(1, 1), 1)),
                        Value(String::from(", ")),
                    ],
                }
            );
        }
        {
            let input = r#"=concat(A1:A, "lol")"#;
            let expr = Expression::parse(input).expect("parsing failed");
            println!("{expr:#?}");
            assert_eq!(
                expr,
                Function {
                    name: String::from("concat"),
                    inputs: vec![
                        Expression::Reference(Reference::UnboundedColRange(CellPointer(1, 1), 1)),
                        Value(String::from("lol")),
                    ],
                }
            );
        }
    }

    #[test]
    fn test_parse_expression_two_commas() {
        let input = "=add(2,, 4)";
        let expr = Expression::parse(input);
        expr.expect_err("parsing ok");
    }

    #[test]
    fn test_parse_expression_unclosed_bracket() {
        {
            let input = "=add(2, 4";
            let expr = Expression::parse(input);
            expr.expect_err("parsing ok");
        }
        {
            let input = "=add(2, 4,";
            let expr = Expression::parse(input);
            expr.expect_err("parsing ok");
        }
    }
}
