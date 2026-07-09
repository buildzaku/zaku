use anyhow::Context;
use jsonc_parser::cst::{
    CstContainerNode, CstInputValue, CstLeafNode, CstNode, CstObject, CstObjectProp, CstRootNode,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

use settings_content::JSONC_PARSE_OPTIONS;

pub fn parse_jsonc<T: DeserializeOwned>(content: &str) -> anyhow::Result<T> {
    Ok(jsonc_parser::parse_to_serde_value::<T>(
        content,
        &JSONC_PARSE_OPTIONS,
    )?)
}

pub fn update_jsonc_content(
    content: &str,
    old_value: &Value,
    new_value: &Value,
) -> anyhow::Result<Option<String>> {
    let content = if content.trim().is_empty() {
        "{}"
    } else {
        content
    };
    let root_node = CstRootNode::parse(content, &JSONC_PARSE_OPTIONS)
        .context("jsonc could not be parsed; fix syntax errors before updating")?;
    let root_value = root_node
        .value()
        .context("jsonc could not be parsed; missing value")?;

    update_value_in_jsonc_node(root_value, old_value, new_value)?;

    let new_content = root_node.to_string();
    if new_content == content {
        return Ok(None);
    }

    Ok(Some(new_content))
}

pub fn move_property_at_jsonc_path(
    content: &str,
    from: &str,
    to: &str,
) -> anyhow::Result<Option<String>> {
    let from_segments = path_segments(from)?;
    let to_segments = path_segments(to)?;

    if from_segments == to_segments {
        anyhow::bail!("jsonc source and destination paths cannot be the same");
    }
    if to_segments.starts_with(from_segments.as_slice()) {
        anyhow::bail!("jsonc destination path cannot be inside source path");
    }
    if from_segments.starts_with(to_segments.as_slice()) {
        anyhow::bail!("jsonc source path cannot be inside destination path");
    }

    let content = if content.trim().is_empty() {
        "{}"
    } else {
        content
    };
    let root_node = CstRootNode::parse(content, &JSONC_PARSE_OPTIONS)
        .context("jsonc could not be parsed; fix syntax errors before updating")?;
    let root_object = root_node
        .object_value_or_create()
        .context("jsonc root must be an object")?;

    let Some(source_property) = property_at_path(&root_object, &from_segments) else {
        return Ok(None);
    };
    let source_value = source_property
        .value()
        .and_then(|value| value.to_serde_value())
        .context("jsonc source property could not be converted to a value")?;

    let Some((&destination_key, destination_parent_segments)) = to_segments.split_last() else {
        anyhow::bail!("jsonc destination path cannot be empty");
    };

    let mut destination_parent = root_object.clone();
    for segment in destination_parent_segments {
        let segment = *segment;
        if let Some(property) = destination_parent.get(segment) {
            let Some(value) = property.value() else {
                anyhow::bail!("jsonc path parent is missing a value: {segment}");
            };
            let CstNode::Container(CstContainerNode::Object(next_object)) = value else {
                anyhow::bail!("jsonc path parent is not an object: {segment}");
            };

            destination_parent = next_object;
        } else {
            let property = destination_parent.append(segment, CstInputValue::Object(Vec::new()));
            destination_parent = property.object_value_or_set();
        }
    }

    if destination_parent.get(destination_key).is_none() {
        destination_parent.append(destination_key, cst_value_from_json(&source_value));
    }

    source_property.remove();

    let new_content = root_node.to_string();
    if new_content == content {
        return Ok(None);
    }

    Ok(Some(new_content))
}

pub fn remove_property_at_jsonc_path(content: &str, path: &str) -> anyhow::Result<Option<String>> {
    let segments = path_segments(path)?;

    let content = if content.trim().is_empty() {
        "{}"
    } else {
        content
    };
    let root_node = CstRootNode::parse(content, &JSONC_PARSE_OPTIONS)
        .context("jsonc could not be parsed; fix syntax errors before updating")?;
    let root_object = root_node
        .object_value_or_create()
        .context("jsonc root must be an object")?;

    let Some(property) = property_at_path(&root_object, &segments) else {
        return Ok(None);
    };

    property.remove();

    let new_content = root_node.to_string();
    if new_content == content {
        return Ok(None);
    }

    Ok(Some(new_content))
}

fn path_segments(path: &str) -> anyhow::Result<Vec<&str>> {
    if path.is_empty() {
        anyhow::bail!("jsonc path cannot be empty");
    }

    let segments = path.split('.').collect::<Vec<_>>();
    if segments.iter().any(|segment| segment.is_empty()) {
        anyhow::bail!("invalid jsonc path: {path}");
    }

    Ok(segments)
}

fn property_at_path(root_object: &CstObject, path: &[&str]) -> Option<CstObjectProp> {
    let (&property_key, parent_segments) = path.split_last()?;
    let mut object = root_object.clone();

    for segment in parent_segments {
        let segment = *segment;
        let property = object.get(segment)?;
        let value = property.value()?;
        let CstNode::Container(CstContainerNode::Object(next_object)) = value else {
            return None;
        };

        object = next_object;
    }

    object.get(property_key)
}

fn update_value_in_jsonc_node(
    node: CstNode,
    old_value: &Value,
    new_value: &Value,
) -> anyhow::Result<()> {
    if let (Value::Object(old_object), Value::Object(new_object)) = (old_value, new_value) {
        let CstNode::Container(CstContainerNode::Object(object)) = &node else {
            return replace_jsonc_node(node, cst_value_from_json(new_value));
        };

        for (old_key, _) in old_object {
            if !new_object.contains_key(old_key)
                && let Some(property) = object.get(old_key)
            {
                property.remove();
            }
        }

        for (new_key_index, (new_key, new_property_value)) in new_object.iter().enumerate() {
            let old_property_value = old_object.get(new_key);

            match (object.get(new_key), old_property_value) {
                (Some(property), Some(old_property_value)) => {
                    if let Some(value) = property.value() {
                        update_value_in_jsonc_node(value, old_property_value, new_property_value)?;
                    } else {
                        property.set_value(cst_value_from_json(new_property_value));
                    }
                }
                (Some(property), None) => {
                    property.set_value(cst_value_from_json(new_property_value));
                }
                (None, _) => {
                    let insert_index = new_key_index.min(object.properties().len());
                    object.insert(
                        insert_index,
                        new_key,
                        cst_value_from_json(new_property_value),
                    );
                }
            }
        }

        return Ok(());
    }

    if old_value != new_value {
        replace_jsonc_node(node, cst_value_from_json(new_value))?;
    }

    Ok(())
}

fn cst_value_from_json(value: &Value) -> CstInputValue {
    match value {
        Value::Null => CstInputValue::Null,
        Value::Bool(value) => CstInputValue::Bool(*value),
        Value::Number(value) => CstInputValue::Number(value.to_string()),
        Value::String(value) => CstInputValue::String(value.clone()),
        Value::Array(values) => {
            CstInputValue::Array(values.iter().map(cst_value_from_json).collect())
        }
        Value::Object(properties) => CstInputValue::Object(
            properties
                .iter()
                .map(|(name, value)| (name.clone(), cst_value_from_json(value)))
                .collect(),
        ),
    }
}

fn replace_jsonc_node(node: CstNode, value: CstInputValue) -> anyhow::Result<()> {
    match node {
        CstNode::Container(CstContainerNode::Root(root)) => root.set_value(value),
        CstNode::Container(CstContainerNode::Object(object)) => {
            object.replace_with(value);
        }
        CstNode::Container(CstContainerNode::ObjectProp(property)) => {
            property.set_value(value);
        }
        CstNode::Container(CstContainerNode::Array(array)) => {
            array.replace_with(value);
        }
        CstNode::Leaf(CstLeafNode::NullKeyword(value_node)) => {
            value_node.replace_with(value);
        }
        CstNode::Leaf(CstLeafNode::BooleanLit(value_node)) => {
            value_node.replace_with(value);
        }
        CstNode::Leaf(CstLeafNode::NumberLit(value_node)) => {
            value_node.replace_with(value);
        }
        CstNode::Leaf(CstLeafNode::StringLit(value_node)) => {
            value_node.replace_with(value);
        }
        CstNode::Leaf(CstLeafNode::WordLit(value_node)) => {
            value_node.replace_with(value);
        }
        CstNode::Leaf(
            CstLeafNode::Token(_)
            | CstLeafNode::Whitespace(_)
            | CstLeafNode::Newline(_)
            | CstLeafNode::Comment(_),
        ) => {
            anyhow::bail!("failed to update JSONC: unexpected trivia")
        }
    }

    Ok(())
}
