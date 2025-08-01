use crate::reference::Reference;

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

const EQUAL_SIGN: char = '=';
const COMMA: char = ',';
const OPENING_BRACKET: char = '(';
const CLOSING_BRACKET: char = ')';

// TODO: Support strings.

impl Expression {
    pub fn parse(mut input: &str) -> Result<Expression, &'static str> {
        // TODO: Change to test_log.
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
                let expr = Self::parse(&taken)?;
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
                let expr = Self::parse(&taken)?;
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
