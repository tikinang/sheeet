use std::collections::HashMap;
use std::ops::Index;

#[derive(Debug, PartialEq)]
pub struct CellPointer(usize, usize);

pub struct Cell {
    parents: HashMap<CellPointer, Cell>,
    children: HashMap<CellPointer, Cell>,
    comp_val: Option<String>,
    expr: Option<Expression>,
}

/// =add(A, sub(4, 2))
#[derive(Debug, PartialEq)]
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

fn column_name_to_usize(name: &str) -> usize {
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
    index
}

fn usize_to_column_name(mut index: usize) -> String {
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
#[derive(Debug, PartialEq)]
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
                        Expression::Reference(Reference::Single(CellPointer(0, 2))),
                        Expression::Reference(Reference::UnboundedColRange(CellPointer(0, 0), 0)),
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
        assert_eq!(column_name_to_usize("A"), 0);
        assert_eq!(column_name_to_usize("a"), 0);
        assert_eq!(column_name_to_usize("Z"), 25);
        assert_eq!(column_name_to_usize("AA"), 26);
        assert_eq!(column_name_to_usize("AAB"), 53);

        assert_eq!(usize_to_column_name(0), "a");
        assert_eq!(usize_to_column_name(25), "z");
        assert_eq!(usize_to_column_name(26), "aa");
        assert_eq!(usize_to_column_name(53), "aab");

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
            Reference::Single(CellPointer(0, 1))
        );
        assert_eq!(
            Reference::parse("A0").unwrap(),
            Reference::Single(CellPointer(0, 0))
        );
        assert_eq!(
            Reference::parse("A1:A5").unwrap(),
            Reference::BoundedRange(CellPointer(0, 1), CellPointer(0, 5))
        );
        assert_eq!(
            Reference::parse("A1:B5").unwrap(),
            Reference::BoundedRange(CellPointer(0, 1), CellPointer(1, 5))
        );
        assert_eq!(
            Reference::parse("A1:A").unwrap(),
            Reference::UnboundedColRange(CellPointer(0, 1), 0)
        );
        assert_eq!(
            Reference::parse("A1:1").unwrap(),
            Reference::UnboundedRowRange(CellPointer(0, 1), 1)
        );
        assert_eq!(
            Reference::parse("A100:AB150").unwrap(),
            Reference::BoundedRange(CellPointer(0, 100), CellPointer(27, 150))
        );

        Reference::parse("1").expect_err("expected err");
        Reference::parse("1A").expect_err("expected err");
        Reference::parse("A1A").expect_err("expected err");
        Reference::parse("A1:1A").expect_err("expected err");
        Reference::parse("A1::").expect_err("expected err");
        Reference::parse("-").expect_err("expected err");
    }
}
