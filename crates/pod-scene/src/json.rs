use serde::Serialize;
use serde_json::Value;

pub(crate) fn to_stable_json_string<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let value = to_stable_json_value(value)?;
    serde_json::to_string_pretty(&value)
}

pub(crate) fn to_stable_json_value<T: Serialize>(value: &T) -> Result<Value, serde_json::Error> {
    let mut value = serde_json::to_value(value)?;
    stabilize_json_value(&mut value);
    Ok(value)
}

pub(crate) fn stabilize_json_value(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = std::mem::take(map).into_iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            for (_, child) in &mut entries {
                stabilize_json_value(child);
            }
            map.extend(entries);
        }
        Value::Array(items) => {
            for item in items {
                stabilize_json_value(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::to_stable_json_string;
    use serde::Serialize;
    use std::collections::HashMap;

    #[derive(Serialize)]
    struct Sample {
        fields: HashMap<String, u32>,
        nested: serde_json::Value,
    }

    #[test]
    fn stable_json_string_is_independent_of_hashmap_insertion_order() {
        let mut left_fields = HashMap::new();
        left_fields.insert("beta".to_string(), 2);
        left_fields.insert("alpha".to_string(), 1);

        let mut right_fields = HashMap::new();
        right_fields.insert("alpha".to_string(), 1);
        right_fields.insert("beta".to_string(), 2);

        let left = Sample {
            fields: left_fields,
            nested: serde_json::json!({
                "z": {"b": true, "a": false},
                "a": 3
            }),
        };
        let right = Sample {
            fields: right_fields,
            nested: serde_json::json!({
                "a": 3,
                "z": {"a": false, "b": true}
            }),
        };

        assert_eq!(
            to_stable_json_string(&left).unwrap(),
            to_stable_json_string(&right).unwrap()
        );
    }
}
