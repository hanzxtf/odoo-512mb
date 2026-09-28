//! The request shape, the response shapes, and the error mapping, against a
//! stub server. No Odoo needed.

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use hodoo::{
    Client, Config, Error, Id, Priority, ProjectId, TagId, TaskFields, TaskFilter, TaskId,
    TaskState, UserId,
};
use serde_json::{Value, json};
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// A client pointed at the stub server.
fn client(server: &MockServer, api_key: &str) -> Client {
    let config = Config::new(server.uri(), api_key)
        .expect("config")
        .with_timeout(Duration::from_secs(5));
    Client::new(config).expect("client")
}

/// A task as Odoo 19 answers a `search_read`: many2one as a pair, x2many as
/// numbers, datetimes as naive UTC.
fn odoo_task() -> Value {
    json!([{
        "id": 9,
        "name": "Ship the release",
        "description": false,
        "project_id": [3, "Website"],
        "stage_id": [11, "In Progress"],
        "state": "01_in_progress",
        "priority": "2",
        "user_ids": [1, 7],
        "partner_id": false,
        "date_deadline": "2026-09-28 14:30:00",
        "date_assign": false,
        "date_last_stage_update": "2026-09-27 09:00:00",
        "allocated_hours": 4.5,
        "tag_ids": [[4, "urgent"]],
        "parent_id": false,
        "milestone_id": [2, "Beta"],
        "is_closed": false
    }])
}

#[tokio::test]
async fn a_search_sends_named_arguments_and_decodes_odoo_shapes() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/search_read"))
        .and(header("authorization", "Bearer secret"))
        .and(header("content-type", "application/json"))
        .and(body_partial_json(json!({
            "domain": [["project_id", "=", 3], ["is_closed", "=", false]],
            "fields": hodoo::task::FIELDS,
            "limit": 10,
            "order": "priority desc, id asc"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(odoo_task()))
        .mount(&server)
        .await;

    let tasks = client(&server, "secret")
        .tasks()
        .search(TaskFilter {
            project: Some(ProjectId::new(3)),
            open_only: true,
            limit: Some(10),
            order: Some("priority desc, id asc".into()),
            ..TaskFilter::default()
        })
        .await
        .expect("search");

    assert_eq!(tasks.len(), 1);
    let task = &tasks[0];
    assert_eq!(task.id, TaskId::new(9));
    assert_eq!(task.project, Some(ProjectId::new(3)));
    assert_eq!(task.stage, Some(hodoo::TaskStageId::new(11)));
    assert_eq!(task.assignees, vec![UserId::new(1), UserId::new(7)]);
    assert_eq!(task.tags, vec![TagId::new(4)]);
    assert_eq!(task.milestone, Some(hodoo::MilestoneId::new(2)));
    assert_eq!(task.priority, Priority::High);
    assert_eq!(task.state, TaskState::InProgress);
    assert_eq!(task.customer, None, "false means no customer");
    assert_eq!(task.description, None, "false means no description");
    assert_eq!(
        task.deadline.expect("deadline").to_string(),
        "2026-09-28 14:30:00 UTC"
    );
    assert_eq!(task.allocated_hours, Some(4.5));
    assert!(!task.is_closed);
}

#[tokio::test]
async fn a_create_sends_vals_list_with_odoo_collection_commands() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/create"))
        .and(body_partial_json(json!({
            "vals_list": [{
                "name": "Ship the release",
                "project_id": 3,
                "stage_id": 11,
                "state": "1_done",
                "priority": "3",
                "user_ids": [[6, 0, [1, 2]]],
                "tag_ids": [[6, 0, []]],
                "date_deadline": "2026-09-28 14:30:00",
                "allocated_hours": 4.0
            }]
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([42])))
        .mount(&server)
        .await;

    let id = client(&server, "secret")
        .tasks()
        .create(TaskFields {
            name: Some("Ship the release".into()),
            project: Some(ProjectId::new(3)),
            stage: Some(hodoo::TaskStageId::new(11)),
            state: Some(TaskState::Done),
            priority: Some(Priority::Urgent),
            assignees: Some(vec![UserId::new(1), UserId::new(2)]),
            tags: Some(Vec::new()),
            deadline: Some(hodoo::datetime::parse_datetime("2026-09-28 14:30:00").expect("stamp")),
            allocated_hours: Some(4.0),
            ..TaskFields::default()
        })
        .await
        .expect("create");

    assert_eq!(id, TaskId::new(42), "create answers a list of ids");
}

#[tokio::test]
async fn unset_fields_are_not_sent_so_a_write_never_clears_them() {
    let server = MockServer::start().await;
    let body = Arc::new(Mutex::new(None));

    let recorder = {
        let body = Arc::clone(&body);
        move |request: &Request| {
            *body.lock().expect("lock") = request.body_json::<Value>().ok();
            ResponseTemplate::new(200).set_body_json(json!(true))
        }
    };
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/write"))
        .respond_with(recorder)
        .mount(&server)
        .await;

    client(&server, "secret")
        .tasks()
        .update(
            TaskId::new(9),
            TaskFields {
                state: Some(TaskState::Canceled),
                ..TaskFields::default()
            },
        )
        .await
        .expect("update");

    let sent = body.lock().expect("lock").clone().expect("a body");
    assert_eq!(
        sent,
        json!({ "ids": [9], "vals": { "state": "1_canceled" } })
    );
}

#[tokio::test]
async fn the_database_header_is_sent_only_when_it_is_configured() {
    let server = MockServer::start().await;
    let seen = Arc::new(Mutex::new(Vec::new()));

    let recorder = {
        let seen = Arc::clone(&seen);
        move |request: &Request| {
            let value = request
                .headers
                .get("x-odoo-database")
                .map(|v| v.to_str().unwrap_or_default().to_owned());
            seen.lock().expect("lock").push(value);
            ResponseTemplate::new(200).set_body_json(json!([{ "id": 1, "name": "Website" }]))
        }
    };
    Mock::given(method("POST"))
        .and(path("/json/2/project.project/read"))
        .respond_with(recorder)
        .mount(&server)
        .await;

    client(&server, "secret")
        .projects()
        .get(ProjectId::new(1))
        .await
        .expect("read");

    let with_db = Client::new(
        Config::new(server.uri(), "secret")
            .expect("config")
            .with_db("odoo"),
    )
    .expect("client");
    with_db
        .projects()
        .get(ProjectId::new(1))
        .await
        .expect("read");

    let seen = seen.lock().expect("lock").clone();
    assert_eq!(seen, vec![None, Some("odoo".to_owned())]);
}

#[tokio::test]
async fn version_needs_no_api_key() {
    let server = MockServer::start().await;
    let authorization = Arc::new(Mutex::new(Some("unset".to_owned())));

    let recorder = {
        let authorization = Arc::clone(&authorization);
        move |request: &Request| {
            *authorization.lock().expect("lock") = request
                .headers
                .get("authorization")
                .map(|value| value.to_str().unwrap_or_default().to_owned());
            ResponseTemplate::new(200).set_body_json(json!({
                "version_info": [19, 0, 0, "final", 0, ""],
                "version": "19.0"
            }))
        }
    };
    Mock::given(method("GET"))
        .and(path("/web/version"))
        .respond_with(recorder)
        .mount(&server)
        .await;

    let version = client(&server, "").version().await.expect("version");
    assert_eq!(version, "19.0");
    assert_eq!(
        *authorization.lock().expect("lock"),
        None,
        "no key was sent"
    );
}

#[tokio::test]
async fn odoos_error_object_becomes_an_odoo_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/search_read"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "name": "werkzeug.exceptions.Unauthorized",
            "message": "Invalid apikey",
            "arguments": ["Invalid apikey", 401],
            "context": {},
            "debug": "Traceback (most recent call last): ..."
        })))
        .mount(&server)
        .await;

    let error = client(&server, "wrong")
        .tasks()
        .search(TaskFilter::default())
        .await
        .expect_err("401");

    assert!(error.is_unauthorized());
    assert_eq!(error.status(), Some(401));
    assert_eq!(error.kind(), "odoo");
    assert!(error.to_string().contains("Invalid apikey"));
    match error {
        Error::Odoo { name, debug, .. } => {
            assert_eq!(name, "werkzeug.exceptions.Unauthorized");
            assert!(debug.expect("traceback").contains("Traceback"));
        }
        other => panic!("expected an Odoo error, got {other:?}"),
    }
}

#[tokio::test]
async fn an_unknown_method_is_reported_as_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/not_a_method"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({
            "name": "werkzeug.exceptions.NotFound",
            "message": "The method not_a_method does not exist on project.task",
            "arguments": ["The method not_a_method does not exist on project.task", 404],
            "context": {},
            "debug": null
        })))
        .mount(&server)
        .await;

    let error = client(&server, "secret")
        .call("project.task", "not_a_method", json!({}))
        .await
        .expect_err("404");

    assert!(error.is_not_found());
    assert_eq!(error.status(), Some(404));
}

#[tokio::test]
async fn an_html_502_is_not_mistaken_for_an_odoo_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/search_read"))
        .respond_with(ResponseTemplate::new(502).set_body_string("<html>502 Bad Gateway</html>"))
        .mount(&server)
        .await;

    let error = client(&server, "secret")
        .tasks()
        .search(TaskFilter::default())
        .await
        .expect_err("502");

    match error {
        Error::UnexpectedResponse { status, body } => {
            assert_eq!(status, 502);
            assert!(body.contains("Bad Gateway"));
        }
        other => panic!("expected an unexpected response, got {other:?}"),
    }
}

#[tokio::test]
async fn a_success_that_is_not_json_is_reported_rather_than_decoded() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.project/read"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;

    let error = client(&server, "secret")
        .projects()
        .get(ProjectId::new(1))
        .await
        .expect_err("not json");

    assert!(matches!(
        error,
        Error::UnexpectedResponse { status: 200, .. }
    ));
}

#[tokio::test]
async fn a_body_that_does_not_fit_the_model_is_a_decode_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/read"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{ "id": "not a number" }])))
        .mount(&server)
        .await;

    let error = client(&server, "secret")
        .tasks()
        .get(TaskId::new(9))
        .await
        .expect_err("bad id");

    match error {
        Error::Decode { body, .. } => assert_eq!(body[0]["id"], "not a number"),
        other => panic!("expected a decode error, got {other:?}"),
    }
}

#[tokio::test]
async fn a_missing_key_is_a_config_error_before_any_request() {
    let server = MockServer::start().await;
    let error = client(&server, "").whoami().await.expect_err("no key");

    assert!(matches!(error, Error::Config { .. }));
    assert_eq!(error.kind(), "config");
}

#[tokio::test]
async fn a_read_that_meets_a_502_is_retried_once() {
    let server = MockServer::start().await;
    let calls = Arc::new(Mutex::new(0_usize));
    let responder = {
        let calls = Arc::clone(&calls);
        move |_request: &Request| {
            let mut count = calls.lock().expect("lock");
            *count += 1;
            if *count == 1 {
                ResponseTemplate::new(502).set_body_string("<html>Bad Gateway</html>")
            } else {
                ResponseTemplate::new(200).set_body_json(odoo_task())
            }
        }
    };
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/search_read"))
        .respond_with(responder)
        .mount(&server)
        .await;

    let tasks = client(&server, "secret")
        .tasks()
        .search(TaskFilter::default())
        .await
        .expect("the retry succeeds");

    assert_eq!(tasks.len(), 1);
    assert_eq!(*calls.lock().expect("lock"), 2);
}

#[tokio::test]
async fn a_write_that_meets_a_502_is_not_retried() {
    let server = MockServer::start().await;
    let calls = Arc::new(Mutex::new(0_usize));
    let responder = {
        let calls = Arc::clone(&calls);
        move |_request: &Request| {
            let mut count = calls.lock().expect("lock");
            *count += 1;
            ResponseTemplate::new(502).set_body_string("<html>Bad Gateway</html>")
        }
    };
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/create"))
        .respond_with(responder)
        .mount(&server)
        .await;

    client(&server, "secret")
        .tasks()
        .create(TaskFields::new("Twice would be a bug"))
        .await
        .expect_err("502");

    assert_eq!(*calls.lock().expect("lock"), 1, "create is never retried");
}

#[tokio::test]
async fn ids_are_tagged_so_a_task_id_cannot_be_passed_as_a_project_id() {
    // A compile-time property: this line would not compile if it were swapped.
    let project: ProjectId = Id::new(3);
    let user: UserId = Id::new(1);
    assert_eq!(project.get(), 3);
    assert_eq!(user.get(), 1);
    assert_eq!(format!("{project:?}"), "Project(3)");
}
