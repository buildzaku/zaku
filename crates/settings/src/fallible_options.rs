use serde::Deserialize;
use std::cell::RefCell;

thread_local! {
    static ERRORS: RefCell<Option<Vec<anyhow::Error>>> = const { RefCell::new(None) };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseStatus {
    Success,
    Failed { error: String },
}

pub fn parse_json<'de, T>(json: &'de str) -> (Option<T>, ParseStatus)
where
    T: Deserialize<'de>,
{
    ERRORS.with_borrow_mut(|errors| {
        errors.replace(Vec::default());
    });

    let mut deserializer = serde_json::Deserializer::from_str(json);
    let value = T::deserialize(&mut deserializer);
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
                errors.push(anyhow::anyhow!("{}", error));
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
    use settings_macros::with_fallible_options;

    #[with_fallible_options]
    #[derive(Debug, Deserialize, PartialEq)]
    struct Foo {
        foo: Option<String>,
        bar: Option<usize>,
        baz: Option<bool>,
    }

    #[test]
    fn test_fallible() {
        let input = indoc! {r#"
            {
                "foo": "bar",
                "bar": "foo",
                "baz": 3
            }
        "#};

        let (value, parse_status) = parse_json::<Foo>(input);
        let value = value.expect("Expected partial settings value");
        let ParseStatus::Failed { error } = parse_status else {
            panic!("Expected parse to fail")
        };

        assert_eq!(
            value,
            Foo {
                foo: Some("bar".into()),
                bar: None,
                baz: None,
            }
        );
        assert_eq!(
            error,
            "invalid type: string \"foo\", expected usize at line 3 column 16\ninvalid type: integer `3`, expected a boolean at line 4 column 12".to_string()
        );
    }
}
