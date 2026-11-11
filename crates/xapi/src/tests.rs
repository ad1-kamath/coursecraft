use serde_json::{Value, json};

use crate::package::{PackageSpec, activity_id_for, cmi5_xml, tincan_xml};
use crate::{AnsweredInput, Flavor, LaunchError, LaunchParams, StatementContext};

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn query(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", encode(key), encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn actor() -> &'static str {
    r#"{"mbox":"mailto:ada@example.com","name":"Ada"}"#
}

fn cmi5_query() -> String {
    query(&[
        ("endpoint", "https://lrs.example.com/"),
        ("fetch", "https://lms.example.com/fetch"),
        ("actor", actor()),
        ("registration", "reg-1"),
        ("activityId", "urn:act"),
    ])
}

fn xapi_query() -> String {
    query(&[
        ("endpoint", "https://lrs.example.com"),
        ("actor", actor()),
        ("registration", "reg-1"),
        ("activity_id", "urn:act"),
        ("auth", "Basic c2VrcmV0"),
    ])
}

fn cmi5_params() -> LaunchParams {
    LaunchParams::parse(&cmi5_query(), Flavor::Cmi5).expect("valid cmi5 launch")
}

fn xapi_params() -> LaunchParams {
    LaunchParams::parse(&xapi_query(), Flavor::Xapi).expect("valid xAPI launch")
}

fn context(flavor: Flavor) -> StatementContext {
    let params = match flavor {
        Flavor::Cmi5 => cmi5_params(),
        Flavor::Xapi => xapi_params(),
    };
    StatementContext {
        flavor,
        actor: params.actor,
        activity_id: params.activity_id.unwrap_or_default(),
        activity_name: "Course".into(),
        registration: params.registration,
        context_template: None,
        mastery_score: Some(0.8),
    }
}

fn object() -> Value {
    json!({
        "objectType": "Activity",
        "id": "urn:act",
        "definition": {
            "name": { "en-US": "Course" },
            "type": "http://adlnet.gov/expapi/activities/course"
        }
    })
}

fn verb(name: &str) -> Value {
    json!({
        "id": format!("http://adlnet.gov/expapi/verbs/{name}"),
        "display": { "en-US": name }
    })
}

fn actor_value() -> Value {
    json!({ "mbox": "mailto:ada@example.com", "name": "Ada" })
}

#[test]
fn flavor_ids_round_trip() {
    assert_eq!(Flavor::Cmi5.as_id(), "cmi5");
    assert_eq!(Flavor::Xapi.as_id(), "xapi");
    assert_eq!(Flavor::from_id("cmi5"), Some(Flavor::Cmi5));
    assert_eq!(Flavor::from_id("xapi"), Some(Flavor::Xapi));
    assert_eq!(Flavor::from_id("scorm12"), None);
}

#[test]
fn launch_params_parse_cmi5_with_and_without_question_mark() {
    let plain = cmi5_params();
    let with_mark =
        LaunchParams::parse(&format!("?{}", cmi5_query()), Flavor::Cmi5).expect("leading ?");
    assert_eq!(plain, with_mark);
    assert_eq!(plain.endpoint, "https://lrs.example.com/");
    assert_eq!(plain.registration.as_deref(), Some("reg-1"));
    assert_eq!(plain.activity_id.as_deref(), Some("urn:act"));
    assert_eq!(
        plain.fetch.as_deref(),
        Some("https://lms.example.com/fetch")
    );
    assert_eq!(plain.auth, None);
}

#[test]
fn launch_params_parse_xapi_and_normalise_endpoint() {
    let params = xapi_params();
    assert_eq!(params.endpoint, "https://lrs.example.com/");
    assert_eq!(params.auth.as_deref(), Some("Basic c2VrcmV0"));
    assert_eq!(params.fetch, None);
    assert_eq!(params.activity_id.as_deref(), Some("urn:act"));
}

#[test]
fn launch_params_missing_required_names_the_parameter() {
    let required = [
        ("endpoint", Flavor::Cmi5),
        ("fetch", Flavor::Cmi5),
        ("actor", Flavor::Cmi5),
        ("registration", Flavor::Cmi5),
        ("activityId", Flavor::Cmi5),
        ("endpoint", Flavor::Xapi),
        ("actor", Flavor::Xapi),
        ("auth", Flavor::Xapi),
    ];
    for (missing, flavor) in required {
        let mut pairs: Vec<(&str, &str)> = vec![
            ("endpoint", "https://lrs.example.com"),
            ("fetch", "https://lms.example.com/fetch"),
            ("actor", actor()),
            ("registration", "reg-1"),
            ("activityId", "urn:act"),
            ("auth", "Basic secret"),
        ];
        pairs.retain(|(key, _)| *key != missing);
        let result = LaunchParams::parse(&query(&pairs), flavor);
        assert_eq!(result, Err(LaunchError::Missing(missing)), "{missing}");
        let message = result.expect_err("missing").to_string();
        assert!(message.contains(missing));
    }
}

#[test]
fn launch_params_require_a_json_object_actor() {
    let bad = query(&[
        ("endpoint", "https://lrs.example.com"),
        ("fetch", "https://lms.example.com/fetch"),
        ("actor", "not json"),
        ("registration", "reg-1"),
        ("activityId", "urn:act"),
    ]);
    assert_eq!(
        LaunchParams::parse(&bad, Flavor::Cmi5),
        Err(LaunchError::InvalidActor)
    );

    let array = query(&[
        ("endpoint", "https://lrs.example.com"),
        ("fetch", "https://lms.example.com/fetch"),
        ("actor", "[1,2,3]"),
        ("registration", "reg-1"),
        ("activityId", "urn:act"),
    ]);
    assert_eq!(
        LaunchParams::parse(&array, Flavor::Cmi5),
        Err(LaunchError::InvalidActor)
    );
}

#[test]
fn launch_params_reject_non_http_urls() {
    let bad_endpoint = query(&[
        ("endpoint", "ftp://lrs.example.com"),
        ("fetch", "https://lms.example.com/fetch"),
        ("actor", actor()),
        ("registration", "reg-1"),
        ("activityId", "urn:act"),
    ]);
    assert_eq!(
        LaunchParams::parse(&bad_endpoint, Flavor::Cmi5),
        Err(LaunchError::InvalidUrl("endpoint"))
    );

    let bad_fetch = query(&[
        ("endpoint", "https://lrs.example.com"),
        ("fetch", "javascript:alert(1)"),
        ("actor", actor()),
        ("registration", "reg-1"),
        ("activityId", "urn:act"),
    ]);
    assert_eq!(
        LaunchParams::parse(&bad_fetch, Flavor::Cmi5),
        Err(LaunchError::InvalidUrl("fetch"))
    );
}

#[test]
fn launch_params_tolerate_bad_escapes_and_invalid_utf8() {
    let params = LaunchParams::parse(
        "endpoint=https%3A%2F%2Flrs.example.com&fetch=https%3A%2F%2Flms%2Ffetch&actor=%7B%22mbox%22%3A%22mailto%3Aa%40b%22%7D&registration=bad%zz%FF&activityId=urn%3Aact&plus=a+b",
        Flavor::Cmi5,
    )
    .expect("forgiving parser");
    assert_eq!(params.endpoint, "https://lrs.example.com/");
    assert_eq!(params.registration.as_deref(), Some("bad%zz\u{FFFD}"));
    assert_eq!(params.actor["mbox"], "mailto:a@b");
}

#[test]
fn launch_params_keep_the_first_element_of_array_actor_properties() {
    let params = LaunchParams::parse(
        &query(&[
            ("endpoint", "https://lrs.example.com"),
            ("fetch", "https://lms.example.com/fetch"),
            ("actor", r#"{"name":["First","Second"],"mbox":["mailto:a@b.c"],"account":[{"homePage":"h","name":"n"}]}"#),
            ("registration", "reg-1"),
            ("activityId", "urn:act"),
        ]),
        Flavor::Cmi5,
    )
    .expect("old Tin Can actor");
    assert_eq!(params.actor["name"], "First");
    assert_eq!(params.actor["mbox"], "mailto:a@b.c");
    assert_eq!(params.actor["account"]["name"], "n");
}

#[test]
fn launch_params_debug_hides_auth_and_fetch() {
    let params = LaunchParams::parse(
        &query(&[
            ("endpoint", "https://lrs.example.com"),
            ("fetch", "https://lms.example.com/secret-fetch"),
            ("actor", actor()),
            ("registration", "reg-1"),
            ("activityId", "urn:act"),
            ("auth", "Basic top-secret-token"),
        ]),
        Flavor::Cmi5,
    )
    .expect("params");
    let debug = format!("{params:?}");
    assert!(!debug.contains("secret-fetch"), "{debug}");
    assert!(!debug.contains("top-secret-token"), "{debug}");
    assert!(debug.contains("endpoint"));
}

#[test]
fn launch_params_refuse_an_oversized_query() {
    let huge = "a".repeat(crate::MAX_LAUNCH_QUERY + 1);
    assert_eq!(
        LaunchParams::parse(&huge, Flavor::Cmi5),
        Err(LaunchError::QueryTooLong)
    );
}

#[test]
fn cmi5_initialized_has_cmi5_category_but_no_moveon() {
    let statement = context(Flavor::Cmi5).initialized("id-1", "2020-01-01T00:00:00Z");
    let expected = json!({
        "id": "id-1",
        "timestamp": "2020-01-01T00:00:00Z",
        "actor": actor_value(),
        "verb": verb("initialized"),
        "object": object(),
        "context": {
            "registration": "reg-1",
            "contextActivities": {
                "category": [{ "id": "https://w3id.org/xapi/cmi5/context/categories/cmi5" }]
            }
        }
    });
    assert_eq!(statement, expected);
}

#[test]
fn cmi5_completed_has_both_categories_and_no_score() {
    let statement = context(Flavor::Cmi5).completed("id-1", "ts", "PT1M");
    assert_eq!(
        statement["result"],
        json!({ "completion": true, "duration": "PT1M" })
    );
    let categories = statement["context"]["contextActivities"]["category"]
        .as_array()
        .expect("categories");
    assert_eq!(categories.len(), 2);
    assert_eq!(
        categories[0]["id"],
        "https://w3id.org/xapi/cmi5/context/categories/cmi5"
    );
    assert_eq!(
        categories[1]["id"],
        "https://w3id.org/xapi/cmi5/context/categories/moveon"
    );
    assert!(statement["context"]["extensions"].get("x").is_none());
    assert!(statement["result"].get("success").is_none());
    assert!(statement["result"].get("score").is_none());
}

#[test]
fn cmi5_passed_and_failed_have_exact_results() {
    let ctx = context(Flavor::Cmi5);
    let passed = ctx.passed("id-1", "ts", 0.8, "PT1M");
    assert_eq!(
        passed["result"],
        json!({
            "success": true,
            "score": { "scaled": 0.8 },
            "duration": "PT1M"
        })
    );
    assert_eq!(
        passed["context"]["extensions"]["https://w3id.org/xapi/cmi5/context/extensions/masteryscore"],
        0.8
    );

    let failed = ctx.failed("id-2", "ts", 0.25, "PT2M");
    assert_eq!(
        failed["result"],
        json!({
            "success": false,
            "score": { "scaled": 0.25 },
            "duration": "PT2M"
        })
    );
}

#[test]
fn cmi5_terminated_has_cmi5_but_no_moveon_or_mastery() {
    let statement = context(Flavor::Cmi5).terminated("id-1", "ts", "PT1M");
    assert_eq!(statement["result"], json!({ "duration": "PT1M" }));
    let categories = statement["context"]["contextActivities"]["category"]
        .as_array()
        .expect("categories");
    assert_eq!(categories.len(), 1);
    assert_eq!(
        categories[0]["id"],
        "https://w3id.org/xapi/cmi5/context/categories/cmi5"
    );
    assert!(
        statement["context"]["extensions"]
            .get("https://w3id.org/xapi/cmi5/context/extensions/masteryscore")
            .is_none()
    );
}

#[test]
fn mastery_extension_only_appears_on_passed_and_failed() {
    let ctx = context(Flavor::Cmi5);
    for statement in [
        ctx.initialized("id", "ts"),
        ctx.completed("id", "ts", "PT1S"),
        ctx.terminated("id", "ts", "PT1S"),
    ] {
        assert!(
            statement["context"]["extensions"]
                .get("https://w3id.org/xapi/cmi5/context/extensions/masteryscore")
                .is_none()
        );
    }
    for statement in [
        ctx.passed("id", "ts", 0.9, "PT1S"),
        ctx.failed("id", "ts", 0.1, "PT1S"),
    ] {
        assert_eq!(
            statement["context"]["extensions"]["https://w3id.org/xapi/cmi5/context/extensions/masteryscore"],
            0.8
        );
    }
}

#[test]
fn cmi5_template_is_never_overwritten() {
    let mut ctx = context(Flavor::Cmi5);
    ctx.context_template = Some(json!({
        "contextActivities": {
            "grouping": [{ "id": "urn:group" }],
            "category": [{ "id": "urn:custom" }]
        },
        "extensions": {
            "https://w3id.org/xapi/cmi5/context/extensions/sessionid": "session-1",
            "https://w3id.org/xapi/cmi5/context/extensions/masteryscore": 0.99
        },
        "registration": "template-registration"
    }));

    let passed = ctx.passed("id", "ts", 0.75, "PT1S");
    let context = &passed["context"];
    assert_eq!(context["registration"], "template-registration");
    assert_eq!(
        context["contextActivities"]["grouping"],
        json!([{ "id": "urn:group" }])
    );
    assert_eq!(
        context["extensions"]["https://w3id.org/xapi/cmi5/context/extensions/sessionid"],
        "session-1"
    );
    assert_eq!(
        context["extensions"]["https://w3id.org/xapi/cmi5/context/extensions/masteryscore"],
        0.99
    );
    let categories = context["contextActivities"]["category"]
        .as_array()
        .expect("categories");
    assert_eq!(categories[0]["id"], "urn:custom");
    assert_eq!(
        categories[1]["id"],
        "https://w3id.org/xapi/cmi5/context/categories/cmi5"
    );
    assert_eq!(
        categories[2]["id"],
        "https://w3id.org/xapi/cmi5/context/categories/moveon"
    );
}

#[test]
fn cmi5_answered_keeps_template_without_categories() {
    let mut ctx = context(Flavor::Cmi5);
    ctx.context_template = Some(json!({
        "extensions": {
            "https://w3id.org/xapi/cmi5/context/extensions/sessionid": "session-1"
        }
    }));
    let input = answered_input();
    let statement = ctx.answered("id", "ts", &input);
    let context = &statement["context"];
    assert!(context["contextActivities"].get("category").is_none());
    assert_eq!(
        context["extensions"]["https://w3id.org/xapi/cmi5/context/extensions/sessionid"],
        "session-1"
    );
    assert!(
        context["extensions"]
            .get("https://w3id.org/xapi/cmi5/context/extensions/masteryscore")
            .is_none()
    );
}

fn answered_input() -> AnsweredInput {
    AnsweredInput {
        question_id: "s-q".into(),
        attempt: 2,
        interaction_type: "choice".into(),
        description: "Pick one".into(),
        response: "b[,]c".into(),
        correct_patterns: vec!["b[,]c".into()],
        success: true,
        points: 10,
        awarded: 10,
        survey: false,
    }
}

#[test]
fn answered_has_exact_object_and_result() {
    let statement = context(Flavor::Xapi).answered("id", "ts", &answered_input());
    assert_eq!(
        statement["object"],
        json!({
            "objectType": "Activity",
            "id": "urn:act/questions/s-q",
            "definition": {
                "type": "http://adlnet.gov/expapi/activities/cmi.interaction",
                "interactionType": "choice",
                "description": { "en-US": "Pick one" },
                "correctResponsesPattern": ["b[,]c"]
            }
        })
    );
    assert_eq!(
        statement["result"],
        json!({
            "success": true,
            "response": "b[,]c",
            "score": { "raw": 10, "min": 0, "max": 10 },
            "extensions": { "urn:coursecraft:xapi:attempt": 2 }
        })
    );
    assert_eq!(statement["context"], json!({ "registration": "reg-1" }));
}

#[test]
fn answered_without_points_has_no_score() {
    let mut input = answered_input();
    input.points = 0;
    input.success = false;
    let statement = context(Flavor::Xapi).answered("id", "ts", &input);
    assert!(statement["result"].get("score").is_none());
    assert_eq!(statement["result"]["success"], false);
}

#[test]
fn answered_with_partial_award_reports_raw_and_max() {
    let mut input = answered_input();
    input.success = false;
    input.awarded = 4;
    input.points = 10;
    let statement = context(Flavor::Xapi).answered("id", "ts", &input);
    assert_eq!(statement["result"]["score"]["raw"], 4);
    assert_eq!(statement["result"]["score"]["max"], 10);
}

#[test]
fn plain_xapi_has_no_categories_or_extensions() {
    let ctx = context(Flavor::Xapi);
    for statement in [
        ctx.initialized("id", "ts"),
        ctx.completed("id", "ts", "PT1S"),
        ctx.passed("id", "ts", 0.9, "PT1S"),
        ctx.failed("id", "ts", 0.1, "PT1S"),
        ctx.terminated("id", "ts", "PT1S"),
    ] {
        assert!(statement["context"].get("contextActivities").is_none());
        assert!(statement["context"].get("extensions").is_none());
        assert_eq!(statement["context"], json!({ "registration": "reg-1" }));
    }
}

#[test]
fn plain_xapi_without_registration_has_no_context() {
    let mut ctx = context(Flavor::Xapi);
    ctx.registration = None;
    let statement = ctx.completed("id", "ts", "PT1S");
    assert!(statement.get("context").is_none());
    assert_eq!(
        statement["result"],
        json!({ "completion": true, "duration": "PT1S" })
    );
}

#[test]
fn plain_xapi_completed_is_exact() {
    let expected = json!({
        "id": "id-1",
        "timestamp": "2020-01-01T00:00:00Z",
        "actor": actor_value(),
        "verb": verb("completed"),
        "object": object(),
        "result": { "completion": true, "duration": "PT1M" },
        "context": { "registration": "reg-1" }
    });
    assert_eq!(
        context(Flavor::Xapi).completed("id-1", "2020-01-01T00:00:00Z", "PT1M"),
        expected
    );
}

fn spec() -> PackageSpec {
    PackageSpec {
        course_id: "hello course!".into(),
        title: "A & <B> \"C\"".into(),
        mastery_score: Some(80),
    }
}

#[test]
fn activity_id_sanitizes_course_id() {
    assert_eq!(activity_id_for("a b/c"), "urn:coursecraft:activity:a_b_c");
    assert_eq!(
        activity_id_for("x:y.z-1_2"),
        "urn:coursecraft:activity:x:y.z-1_2"
    );
}

#[test]
fn cmi5_xml_has_required_parts_and_escaping() {
    let xml = cmi5_xml(&spec());
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(xml.contains(
        "<courseStructure xmlns=\"https://w3id.org/xapi/profiles/cmi5/v1/CourseStructure.xsd\">"
    ));
    assert!(xml.contains("<course id=\"urn:coursecraft:course:hello_course_\">"));
    assert!(xml.contains("<au id=\"urn:coursecraft:au:hello_course_\" moveOn=\"Passed\" launchMethod=\"AnyWindow\" masteryScore=\"0.8\">"));
    assert!(xml.contains(
        "<title><langstring lang=\"en-US\">A &amp; &lt;B&gt; &quot;C&quot;</langstring></title>"
    ));
    assert!(xml.contains("<description><langstring lang=\"en-US\">A &amp; &lt;B&gt; &quot;C&quot;</langstring></description>"));
    assert!(xml.contains("<url>index.html</url>"));
}

#[test]
fn cmi5_mastery_formats_and_controls_move_on() {
    for (percent, expected) in [(80, "0.8"), (100, "1"), (75, "0.75"), (5, "0.05"), (0, "0")] {
        let mut spec = spec();
        spec.mastery_score = Some(percent);
        let xml = cmi5_xml(&spec);
        assert!(xml.contains("moveOn=\"Passed\""), "{percent}");
        assert!(
            xml.contains(&format!("masteryScore=\"{expected}\"")),
            "{percent}: {xml}"
        );
    }

    let mut spec = spec();
    spec.mastery_score = None;
    let xml = cmi5_xml(&spec);
    assert!(xml.contains("moveOn=\"Completed\""));
    assert!(!xml.contains("masteryScore"));
}

#[test]
fn tincan_xml_has_required_parts_and_escaping() {
    let xml = tincan_xml(&spec());
    assert!(xml.contains("<tincan xmlns=\"http://projecttincan.com/tincan.xsd\">"));
    assert!(xml.contains(
        "<activity id=\"urn:coursecraft:activity:hello_course_\" type=\"http://adlnet.gov/expapi/activities/course\">"
    ));
    assert!(xml.contains("<name lang=\"en-US\">A &amp; &lt;B&gt; &quot;C&quot;</name>"));
    assert!(
        xml.contains("<description lang=\"en-US\">A &amp; &lt;B&gt; &quot;C&quot;</description>")
    );
    assert!(xml.contains("<launch lang=\"en-US\">index.html</launch>"));
}

#[test]
fn xml_escape_drops_control_characters() {
    let mut spec = spec();
    spec.title = "a\u{0}b\tc".into();
    let xml = tincan_xml(&spec);
    assert!(xml.contains(">ab\tc<"));
}

#[test]
fn statement_scaled_is_clamped_and_never_non_finite() {
    let ctx = context(Flavor::Xapi);
    assert_eq!(
        ctx.passed("id", "ts", f64::NAN, "PT0S")["result"]["score"]["scaled"],
        0.0
    );
    assert_eq!(
        ctx.passed("id", "ts", 4.0, "PT0S")["result"]["score"]["scaled"],
        1.0
    );
}

#[test]
fn launch_params_drop_empty_array_actor_properties() {
    let actor = r#"{"name":["Ada"],"mbox":["mailto:a@b.c"],"mbox_sha1sum":[]}"#;
    let encoded: String = actor.bytes().map(|byte| format!("%{byte:02X}")).collect();
    let query = format!("endpoint=https://lrs.test/&auth=Basic%20x&actor={encoded}");
    let params = LaunchParams::parse(&query, Flavor::Xapi).expect("launch");
    assert_eq!(
        params.actor,
        serde_json::json!({"name":"Ada","mbox":"mailto:a@b.c"})
    );
}

#[test]
fn answered_survey_omits_success_and_score_while_graded_keeps_success() {
    let ctx = context(Flavor::Xapi);

    let mut survey = answered_input();
    survey.survey = true;
    survey.points = 0;
    survey.awarded = 0;
    let statement = ctx.answered("id", "ts", &survey);
    assert!(statement["result"].get("success").is_none());
    assert!(statement["result"].get("score").is_none());
    assert_eq!(statement["result"]["response"], "b[,]c");

    let graded = answered_input();
    assert!(!graded.survey);
    let statement = ctx.answered("id", "ts", &graded);
    assert_eq!(statement["result"]["success"], true);
    assert_eq!(
        statement["result"]["score"],
        json!({ "raw": 10, "min": 0, "max": 10 })
    );
}
