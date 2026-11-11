//! xAPI statement builders shared by the cmi5 and plain xAPI flavors.

use serde_json::{Map, Value, json};

use crate::flavor::Flavor;

const VERB_BASE: &str = "http://adlnet.gov/expapi/verbs/";
const ACTIVITY_TYPE_COURSE: &str = "http://adlnet.gov/expapi/activities/course";
const ACTIVITY_TYPE_INTERACTION: &str = "http://adlnet.gov/expapi/activities/cmi.interaction";
const CMI5_CATEGORY: &str = "https://w3id.org/xapi/cmi5/context/categories/cmi5";
const MOVEON_CATEGORY: &str = "https://w3id.org/xapi/cmi5/context/categories/moveon";
const MASTERY_EXTENSION: &str = "https://w3id.org/xapi/cmi5/context/extensions/masteryscore";

/// A graded answer ready to become an `answered` statement.
#[derive(Debug, Clone, PartialEq)]
pub struct AnsweredInput {
    /// Sanitized question (slide) id.
    pub question_id: String,
    /// How many graded attempts this answer is, starting at 1.
    pub attempt: u32,
    /// xAPI interaction type, e.g. `choice` or `fill-in`.
    pub interaction_type: String,
    /// The question prompt.
    pub description: String,
    /// The learner's response in xAPI interaction vocabulary.
    pub response: String,
    /// Correct responses in xAPI interaction vocabulary.
    pub correct_patterns: Vec<String>,
    /// Whether the response was correct.
    pub success: bool,
    /// The question's point value.
    pub points: u32,
    /// The points the learner earned.
    pub awarded: u32,
    /// Whether this is an ungraded survey answer; its result omits `success`.
    pub survey: bool,
}

/// Everything a statement needs beyond the statement id and timestamp.
#[derive(Debug, Clone)]
pub struct StatementContext {
    pub flavor: Flavor,
    pub actor: Value,
    pub activity_id: String,
    pub activity_name: String,
    pub registration: Option<String>,
    /// The cmi5 `LMS.LaunchData` context template, used only for cmi5.
    pub context_template: Option<Value>,
    /// Passing score from `0.0` to `1.0`, when the LMS supplies one.
    pub mastery_score: Option<f64>,
}

impl StatementContext {
    /// Build an `initialized` statement.
    #[must_use]
    pub fn initialized(&self, id: &str, timestamp: &str) -> Value {
        self.statement(id, timestamp, "initialized", None, &[CMI5_CATEGORY], false)
    }

    /// Build an `answered` statement.
    #[must_use]
    pub fn answered(&self, id: &str, timestamp: &str, input: &AnsweredInput) -> Value {
        let object = json!({
            "objectType": "Activity",
            "id": format!("{}/questions/{}", self.activity_id, input.question_id),
            "definition": {
                "type": ACTIVITY_TYPE_INTERACTION,
                "interactionType": input.interaction_type,
                "description": { "en-US": input.description },
                "correctResponsesPattern": input.correct_patterns,
            }
        });

        let mut result = Map::new();
        if !input.survey {
            result.insert("success".into(), json!(input.success));
        }
        result.insert("response".into(), json!(input.response));
        if input.points > 0 {
            result.insert(
                "score".into(),
                json!({
                    "raw": input.awarded,
                    "min": 0,
                    "max": input.points,
                }),
            );
        }
        result.insert(
            "extensions".into(),
            json!({ "urn:coursecraft:xapi:attempt": input.attempt }),
        );

        self.statement_with_object(
            id,
            timestamp,
            "answered",
            object,
            Some(Value::Object(result)),
            &[],
            false,
        )
    }

    /// Build a `completed` statement.
    #[must_use]
    pub fn completed(&self, id: &str, timestamp: &str, duration: &str) -> Value {
        self.statement(
            id,
            timestamp,
            "completed",
            Some(json!({ "completion": true, "duration": duration })),
            &[CMI5_CATEGORY, MOVEON_CATEGORY],
            false,
        )
    }

    /// Build a `passed` statement.
    #[must_use]
    pub fn passed(&self, id: &str, timestamp: &str, scaled: f64, duration: &str) -> Value {
        self.outcome(id, timestamp, "passed", true, scaled, duration)
    }

    /// Build a `failed` statement.
    #[must_use]
    pub fn failed(&self, id: &str, timestamp: &str, scaled: f64, duration: &str) -> Value {
        self.outcome(id, timestamp, "failed", false, scaled, duration)
    }

    /// Build a `terminated` statement.
    #[must_use]
    pub fn terminated(&self, id: &str, timestamp: &str, duration: &str) -> Value {
        self.statement(
            id,
            timestamp,
            "terminated",
            Some(json!({ "duration": duration })),
            &[CMI5_CATEGORY],
            false,
        )
    }

    fn outcome(
        &self,
        id: &str,
        timestamp: &str,
        verb: &str,
        success: bool,
        scaled: f64,
        duration: &str,
    ) -> Value {
        let result = json!({
            "success": success,
            "score": { "scaled": finite_scaled(scaled) },
            "duration": duration,
        });
        self.statement(
            id,
            timestamp,
            verb,
            Some(result),
            &[CMI5_CATEGORY, MOVEON_CATEGORY],
            true,
        )
    }

    fn statement(
        &self,
        id: &str,
        timestamp: &str,
        verb: &str,
        result: Option<Value>,
        categories: &[&str],
        mastery: bool,
    ) -> Value {
        self.statement_with_object(
            id,
            timestamp,
            verb,
            self.object(),
            result,
            categories,
            mastery,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn statement_with_object(
        &self,
        id: &str,
        timestamp: &str,
        verb: &str,
        object: Value,
        result: Option<Value>,
        categories: &[&str],
        mastery: bool,
    ) -> Value {
        let mut statement = Map::new();
        statement.insert("id".into(), json!(id));
        statement.insert("timestamp".into(), json!(timestamp));
        statement.insert("actor".into(), self.actor.clone());
        statement.insert(
            "verb".into(),
            json!({
                "id": format!("{VERB_BASE}{verb}"),
                "display": { "en-US": verb },
            }),
        );
        statement.insert("object".into(), object);
        if let Some(result) = result {
            statement.insert("result".into(), result);
        }
        if let Some(context) = self.context(categories, mastery) {
            statement.insert("context".into(), context);
        }
        Value::Object(statement)
    }

    fn object(&self) -> Value {
        json!({
            "objectType": "Activity",
            "id": self.activity_id,
            "definition": {
                "name": { "en-US": self.activity_name },
                "type": ACTIVITY_TYPE_COURSE,
            }
        })
    }

    fn context(&self, categories: &[&str], mastery: bool) -> Option<Value> {
        match self.flavor {
            Flavor::Xapi => self.xapi_context(),
            Flavor::Cmi5 => self.cmi5_context(categories, mastery),
        }
    }

    fn xapi_context(&self) -> Option<Value> {
        let mut context = Map::new();
        if let Some(registration) = &self.registration {
            context.insert("registration".into(), json!(registration));
        }
        if context.is_empty() {
            None
        } else {
            Some(Value::Object(context))
        }
    }

    fn cmi5_context(&self, categories: &[&str], mastery: bool) -> Option<Value> {
        let mut context = match &self.context_template {
            Some(Value::Object(template)) => Value::Object(template.clone()),
            _ => Value::Object(Map::new()),
        };
        let object = context.as_object_mut()?;
        if let Some(registration) = &self.registration {
            object
                .entry("registration")
                .or_insert_with(|| json!(registration));
        }
        for category in categories {
            push_context_activity(&mut context, "category", category);
        }
        if mastery && let Some(score) = self.mastery_score.filter(|score| score.is_finite()) {
            push_extension(&mut context, MASTERY_EXTENSION, json!(score));
        }
        Some(context)
    }
}

fn finite_scaled(scaled: f64) -> f64 {
    if scaled.is_finite() {
        scaled.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

fn push_context_activity(context: &mut Value, key: &str, id: &str) {
    let Some(object) = context.as_object_mut() else {
        return;
    };
    let activities = object
        .entry("contextActivities")
        .or_insert_with(|| Value::Object(Map::new()));
    if !activities.is_object() {
        *activities = Value::Object(Map::new());
    }
    let Some(activities) = activities.as_object_mut() else {
        return;
    };

    let existing = activities.get(key).cloned();
    let mut entries = match existing {
        Some(Value::Array(entries)) => entries,
        Some(Value::Object(entry)) => vec![Value::Object(entry)],
        _ => Vec::new(),
    };
    let already_present = entries
        .iter()
        .any(|entry| entry.get("id").and_then(Value::as_str) == Some(id));
    if !already_present {
        entries.push(json!({ "id": id }));
    }
    activities.insert(key.to_string(), Value::Array(entries));
}

fn push_extension(context: &mut Value, key: &str, value: Value) {
    let Some(object) = context.as_object_mut() else {
        return;
    };
    let extensions = object
        .entry("extensions")
        .or_insert_with(|| Value::Object(Map::new()));
    if !extensions.is_object() {
        *extensions = Value::Object(Map::new());
    }
    if let Some(extensions) = extensions.as_object_mut() {
        extensions.entry(key.to_string()).or_insert(value);
    }
}
