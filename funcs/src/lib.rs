use wasm_bindgen::prelude::*;

pub mod prelude {
    pub use crate::add;
    pub use crate::avg;
    pub use crate::concat_with;
    pub use crate::div;
    pub use crate::med;
    pub use crate::mul;
    pub use crate::sub;
    pub use crate::sum;
}

#[wasm_bindgen]
pub fn add(a: f64, b: f64) -> f64 {
    a + b
}

#[wasm_bindgen]
pub fn sub(a: f64, b: f64) -> f64 {
    a - b
}

#[wasm_bindgen]
pub fn div(a: f64, b: f64) -> f64 {
    a / b
}

#[wasm_bindgen]
pub fn mul(a: f64, b: f64) -> f64 {
    a * b
}

#[wasm_bindgen]
pub fn pow(a: f64, n: f64) -> f64 {
    a.powf(n)
}

#[wasm_bindgen]
pub fn sum(vec: Vec<f64>) -> f64 {
    vec.into_iter().sum()
}

#[wasm_bindgen]
pub fn avg(vec: Vec<f64>) -> f64 {
    if vec.len() == 0 {
        return 0.0;
    }
    let len = vec.len() as f64;
    vec.into_iter().sum::<f64>() / len
}

/// Discrete implementation of median.
#[wasm_bindgen]
pub fn med(vec: Vec<f64>) -> f64 {
    if vec.len() == 0 {
        return 0.0;
    }
    vec.get(vec.len() / 2)
        .expect("expected median position to exist in the vector")
        .clone()
}

#[wasm_bindgen]
pub fn concat_with(vec: Vec<String>, sep: &str) -> String {
    let mut str = String::new();
    let len = vec.len();
    for (i, s) in vec.into_iter().enumerate() {
        str.push_str(&s);
        if i < len - 1 {
            str.push_str(sep);
        }
    }
    str
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn test_avg() {
        assert_eq!(avg(vec![]), 0.0);
        assert_eq!(avg(vec![1.0, 2.0, 3.0]), 2.0);
        assert_eq!(avg(vec![1.0, 1.0, 7.0]), 3.0);
    }

    #[test]
    fn test_med() {
        assert_eq!(med(vec![]), 0.0);
        assert_eq!(med(vec![1.0, 2.0, 3.0, 4.0, 5.0]), 3.0);
        assert_eq!(med(vec![1.0, 1.0, 2.0, 2.0, 2.0]), 2.0);
        assert_eq!(med(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]), 4.0); // Discrete implementation.
    }

    #[test]
    fn test_concat_with() {
        assert_eq!(
            concat_with(
                vec![
                    "I".into(),
                    "want".into(),
                    "to".into(),
                    "join".into(),
                    "some".into(),
                    "text.".into()
                ],
                " "
            ),
            "I want to join some text."
        );
        assert_eq!(
            concat_with(vec!["w".into(), "o".into(), "r".into(), "d".into(),], ""),
            "word"
        );
        assert_eq!(
            concat_with(
                vec!["a".into(), "list".into(), "of".into(), "items".into(),],
                ", "
            ),
            "a, list, of, items"
        );
    }
}
