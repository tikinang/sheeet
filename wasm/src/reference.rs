use serde::de::{Error, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
pub struct CellPointer(pub usize, pub usize);

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

#[cfg(test)]
mod tests {
    use super::*;

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
