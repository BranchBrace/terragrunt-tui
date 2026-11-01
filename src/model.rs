use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Totals {
    pub added: usize,
    pub changed: usize,
    pub deleted: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResourceChange {
    pub address: String,
    pub actions: Vec<String>,
    pub before: Value,
    pub after: Value,
}

impl ResourceChange {
    pub fn kind(&self) -> ChangeKind {
        match self.actions.as_slice() {
            [action] if action == "create" => ChangeKind::Add,
            [action] if action == "update" => ChangeKind::Change,
            [action] if action == "delete" => ChangeKind::Delete,
            [first, second]
                if (first == "delete" && second == "create")
                    || (first == "create" && second == "delete") =>
            {
                ChangeKind::Replace
            }
            [action] if action == "read" => ChangeKind::Read,
            [action] if action == "no-op" => ChangeKind::NoChange,
            _ => ChangeKind::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Add,
    Delete,
    Change,
    Replace,
    Read,
    NoChange,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleResult {
    pub path: String,
    pub status: String,
    pub wave: Option<usize>,
    pub totals: Option<Totals>,
    pub resources: Vec<ResourceChange>,
    #[serde(skip)]
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunResult {
    pub schema_version: u32,
    pub command: Vec<String>,
    pub exit_code: i32,
    pub complete: bool,
    #[serde(default)]
    pub cancelled: bool,
    pub totals: Option<Totals>,
    pub modules: Vec<ModuleResult>,
}

#[derive(Debug)]
pub enum PlanError {
    Json(serde_json::Error),
    Invalid(String),
}

impl std::fmt::Display for PlanError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(error) => write!(formatter, "invalid plan JSON: {error}"),
            Self::Invalid(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for PlanError {}

pub fn parse_plan_json(input: &str) -> Result<(Vec<ResourceChange>, Totals), PlanError> {
    let root: Value = serde_json::from_str(input).map_err(PlanError::Json)?;
    if root.get("errored").and_then(Value::as_bool) == Some(true) {
        return Err(PlanError::Invalid("plan JSON reports an error".into()));
    }
    let changes = root
        .get("resource_changes")
        .and_then(Value::as_array)
        .ok_or_else(|| PlanError::Invalid("plan JSON has no resource_changes array".into()))?;

    let mut resources = Vec::with_capacity(changes.len());
    let mut totals = Totals::default();
    for item in changes {
        let address = item
            .get("address")
            .and_then(Value::as_str)
            .unwrap_or("<unknown address>")
            .to_owned();
        let change = item
            .get("change")
            .and_then(Value::as_object)
            .ok_or_else(|| {
                PlanError::Invalid(format!("resource {address} has no change object"))
            })?;
        let actions = change
            .get("actions")
            .and_then(Value::as_array)
            .ok_or_else(|| PlanError::Invalid(format!("resource {address} has no actions array")))?
            .iter()
            .map(|action| {
                action.as_str().map(str::to_owned).ok_or_else(|| {
                    PlanError::Invalid(format!("resource {address} has a non-string action"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let before = change
            .get("before")
            .ok_or_else(|| PlanError::Invalid(format!("resource {address} has no before value")))?;
        let after = change
            .get("after")
            .ok_or_else(|| PlanError::Invalid(format!("resource {address} has no after value")))?;
        if actions.iter().any(|action| action == "create") {
            totals.added += 1;
        }
        if actions.iter().any(|action| action == "update") {
            totals.changed += 1;
        }
        if actions.iter().any(|action| action == "delete") {
            totals.deleted += 1;
        }

        resources.push(ResourceChange {
            address,
            actions,
            before: sanitize(
                before,
                change.get("before_unknown").unwrap_or(&Value::Null),
                change.get("before_sensitive").unwrap_or(&Value::Null),
            ),
            after: sanitize(
                after,
                change.get("after_unknown").unwrap_or(&Value::Null),
                change.get("after_sensitive").unwrap_or(&Value::Null),
            ),
        });
    }
    Ok((resources, totals))
}

fn sanitize(value: &Value, unknown: &Value, sensitive: &Value) -> Value {
    if sensitive == &Value::Bool(true) {
        return Value::String("[REDACTED]".into());
    }
    if unknown == &Value::Bool(true) {
        return Value::String("[UNKNOWN]".into());
    }
    match value {
        Value::Object(object) => {
            let unknown = unknown.as_object();
            let sensitive = sensitive.as_object();
            let mut safe = Map::new();
            for (key, child) in object {
                safe.insert(
                    key.clone(),
                    sanitize(
                        child,
                        unknown
                            .and_then(|mask| mask.get(key))
                            .unwrap_or(&Value::Null),
                        sensitive
                            .and_then(|mask| mask.get(key))
                            .unwrap_or(&Value::Null),
                    ),
                );
            }
            Value::Object(safe)
        }
        Value::Array(items) => {
            let unknown = unknown.as_array();
            let sensitive = sensitive.as_array();
            Value::Array(
                items
                    .iter()
                    .enumerate()
                    .map(|(index, child)| {
                        sanitize(
                            child,
                            unknown
                                .and_then(|mask| mask.get(index))
                                .unwrap_or(&Value::Null),
                            sensitive
                                .and_then(|mask| mask.get(index))
                                .unwrap_or(&Value::Null),
                        )
                    })
                    .collect(),
            )
        }
        _ => value.clone(),
    }
}

pub fn summarize_modules(modules: &[ModuleResult]) -> Option<Totals> {
    let mut totals = Totals::default();
    for module in modules {
        let module_totals = module.totals.as_ref()?;
        totals.added += module_totals.added;
        totals.changed += module_totals.changed;
        totals.deleted += module_totals.deleted;
    }
    Some(totals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_actions_and_redacts_sensitive_and_unknown_values() {
        let input = r#"{
          "resource_changes": [
            {"address":"test_item.added","change":{"actions":["create"],"before":null,"after":{"name":"public","token":"secret"},"after_sensitive":{"token":true}}},
            {"address":"test_item.updated","change":{"actions":["update"],"before":{"size":1},"after":{"size":2,"computed":"x"},"after_unknown":{"computed":true}}},
            {"address":"test_item.replaced","change":{"actions":["delete","create"],"before":{},"after":{}}}
          ]
        }"#;
        let (resources, totals) = parse_plan_json(input).unwrap();
        assert_eq!(
            totals,
            Totals {
                added: 2,
                changed: 1,
                deleted: 1
            }
        );
        assert_eq!(resources[0].after["token"], "[REDACTED]");
        assert_eq!(resources[1].after["computed"], "[UNKNOWN]");
        assert_eq!(resources[2].kind(), ChangeKind::Replace);
    }

    #[test]
    fn malformed_or_errored_plans_fail_closed() {
        assert!(parse_plan_json(r#"{"errored":true,"resource_changes":[]}"#).is_err());
        assert!(parse_plan_json(r#"{"resource_changes":[{"address":"broken"}]}"#).is_err());
    }

    #[test]
    fn unsupported_resource_actions_are_unknown_not_no_change() {
        let input = r#"{"resource_changes":[{"address":"test_item.future","change":{"actions":["future-action"],"before":null,"after":null}}]}"#;
        let (resources, _) = parse_plan_json(input).unwrap();
        assert_eq!(resources[0].kind(), ChangeKind::Unknown);
    }

    #[test]
    fn missing_before_or_after_values_fail_closed() {
        let missing_before = r#"{"resource_changes":[{"address":"test_item.missing","change":{"actions":["create"],"after":{}}}]}"#;
        let missing_after = r#"{"resource_changes":[{"address":"test_item.missing","change":{"actions":["delete"],"before":{}}}]}"#;
        assert!(parse_plan_json(missing_before).is_err());
        assert!(parse_plan_json(missing_after).is_err());
    }

    #[test]
    fn incomplete_module_totals_suppress_global_totals() {
        let complete = ModuleResult {
            path: "a".into(),
            status: "planned".into(),
            wave: Some(0),
            totals: Some(Totals::default()),
            resources: vec![],
            logs: vec![],
        };
        let incomplete = ModuleResult {
            path: "b".into(),
            status: "unknown".into(),
            wave: None,
            totals: None,
            resources: vec![],
            logs: vec![],
        };
        assert_eq!(summarize_modules(&[complete, incomplete]), None);
    }
}
