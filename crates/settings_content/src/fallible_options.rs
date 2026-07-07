use anyhow::anyhow;
use jsonc_parser::ParseOptions;
use serde::de::DeserializeOwned;
use std::cell::RefCell;

use crate::ParseStatus;

thread_local! {
    static ERRORS: RefCell<Option<Vec<anyhow::Error>>> = const { RefCell::new(None) };
}

pub const JSONC_PARSE_OPTIONS: ParseOptions = ParseOptions {
    allow_comments: true,
    allow_loose_object_property_names: false,
    allow_trailing_commas: false,
    allow_missing_commas: false,
    allow_single_quoted_strings: false,
    allow_hexadecimal_numbers: false,
    allow_unary_plus_numbers: false,
};

pub fn parse_jsonc<T>(jsonc: &str) -> (Option<T>, ParseStatus)
where
    T: DeserializeOwned,
{
    ERRORS.with_borrow_mut(|errors| {
        errors.replace(Vec::default());
    });

    let value = jsonc_parser::parse_to_serde_value::<T>(jsonc, &JSONC_PARSE_OPTIONS);
    let value = match value {
        Ok(value) => value,
        Err(error) => {
            return (
                None,
                ParseStatus::Failed {
                    error: error.to_string(),
                },
            );
        }
    };

    if let Some(errors) = ERRORS.with_borrow_mut(|errors| errors.take().filter(|e| !e.is_empty())) {
        let error = errors
            .into_iter()
            .map(|e| e.to_string())
            .flat_map(|e| ["\n".to_owned(), e])
            .skip(1)
            .collect::<String>();
        return (Some(value), ParseStatus::Failed { error });
    }

    (Some(value), ParseStatus::Success)
}

pub(crate) fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de> + FallibleOption,
{
    match T::deserialize(deserializer) {
        Ok(value) => Ok(value),
        Err(error) => ERRORS.with_borrow_mut(|errors| {
            if let Some(errors) = errors {
                errors.push(anyhow!("{error}"));
                Ok(Default::default())
            } else {
                Err(error)
            }
        }),
    }
}

pub trait FallibleOption: Default {}
impl<T> FallibleOption for Option<T> {}

#[cfg(test)]
mod tests {
    use super::*;

    use indoc::indoc;
    use serde::Deserialize;
    use settings_macros::with_fallible_options;

    #[with_fallible_options]
    #[derive(Debug, PartialEq, Deserialize)]
    struct TestSettings {
        string: Option<String>,
        number: Option<usize>,
        boolean: Option<bool>,
    }

    #[test]
    fn test_fallible() {
        let input = indoc! {r#"
            {
              "string": "text",
              "number": "not a number",
              "boolean": 999
            }
        "#};

        let (value, parse_status) = parse_jsonc::<TestSettings>(input);
        let value = value.expect("expected partial settings value");
        let ParseStatus::Failed { error } = parse_status else {
            panic!("expected fallible option errors")
        };

        assert_eq!(
            value,
            TestSettings {
                string: Some("text".into()),
                number: None,
                boolean: None,
            }
        );
        assert!(error.contains("invalid type: string \"not a number\", expected usize"));
        assert!(error.contains("invalid type: integer `999`, expected a boolean"));
    }

    #[test]
    fn test_parse_jsonc_allows_comments() {
        let input = indoc! {r#"
            {
              // Line comment
              "string": "text",
              /*
               * Block comment
               */
              "number": 1,
              "boolean": true
            }
        "#};

        let (value, parse_status) = parse_jsonc::<TestSettings>(input);

        assert_eq!(parse_status, ParseStatus::Success);
        assert_eq!(
            value,
            Some(TestSettings {
                string: Some("text".into()),
                number: Some(1),
                boolean: Some(true),
            })
        );
    }

    #[test]
    fn test_parse_jsonc_uses_strict_options() {
        let inputs = [
            ("loose property names", r#"{ string: "text" }"#),
            ("trailing commas", r#"{ "string": "text", }"#),
            ("missing commas", r#"{ "string": "text" "number": 1 }"#),
            ("single quoted strings", r#"{ "string": 'text' }"#),
            ("hexadecimal numbers", r#"{ "number": 0x10 }"#),
            ("unary plus numbers", r#"{ "number": +1 }"#),
        ];

        for (description, input) in inputs {
            let (value, parse_status) = parse_jsonc::<TestSettings>(input);

            assert_eq!(
                value, None,
                "expected {description} to fail before deserialization"
            );
            let ParseStatus::Failed { error } = parse_status else {
                panic!("expected {description} to fail")
            };
            assert!(
                !error.trim().is_empty(),
                "expected {description} to report an error"
            );
        }
    }
}
