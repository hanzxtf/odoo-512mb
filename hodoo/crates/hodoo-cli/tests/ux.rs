//! The CLI as a user meets it: output modes, refusals, dry runs, exit codes.
//!
//! A stub Odoo stands in for the server, so these run offline and assert on what
//! actually reaches the terminal. `assert_cmd` runs the real binary, which is the only
//! way to check exit codes and the stdout/stderr split.

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use std::sync::{Arc, Mutex};

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{Value, json};
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// A project as Odoo answers a `search_read`.
fn odoo_project() -> Value {
    json!([{
        "id": 49,
        "name": "Acme Manufacturing - Website",
        "description": false,
        "active": true,
        "partner_id": [3, "Acme Manufacturing"],
        "user_id": [2, "Hanz"],
        "stage_id": [2, "In Progress"],
        "date_start": "2026-10-01",
        "date": "2026-12-15",
        "privacy_visibility": "employees",
        "tag_ids": [],
        "type_ids": [11, 12],
        "task_count": 8,
        "open_task_count": 6
    }])
}

/// A task as Odoo answers a `search_read`, with a deadline three days out.
fn odoo_task(due_days_from_today: i64) -> Value {
    let due = (chrono::Utc::now() + chrono::Duration::days(due_days_from_today))
        .format("%Y-%m-%d 09:00:00")
        .to_string();
    json!([{
        "id": 31,
        "name": "Write the launch email",
        "description": "<p>Short and warm.</p>",
        "project_id": [49, "Acme Manufacturing - Website"],
        "stage_id": [11, "Backlog"],
        "state": "01_in_progress",
        "priority": "2",
        "user_ids": [5],
        "partner_id": false,
        "date_deadline": due,
        "date_assign": false,
        "date_last_stage_update": false,
        "allocated_hours": 4.0,
        "tag_ids": [],
        "parent_id": false,
        "milestone_id": false,
        "is_closed": false
    }])
}

/// A client pointed at the stub server, with credentials in the environment.
async fn server() -> MockServer {
    MockServer::start().await
}

/// The same task as Odoo answers after a state write it recomputed: blocked by an
/// open dependency, so the state it computes is "waiting" whatever was written.
fn odoo_waiting_task() -> Value {
    let mut task = odoo_task(3);
    task[0]["state"] = json!("04_waiting_normal");
    task
}

fn hodoo(server: &MockServer) -> Command {
    let mut command = Command::cargo_bin("hodoo").expect("the binary");
    command
        .env("ODOO_URL", server.uri())
        .env("ODOO_API_KEY", "secret")
        .env_remove("HODOO_OUTPUT")
        .env_remove("HODOO_COLOR")
        .env_remove("ODOO_DB");
    command
}

#[tokio::test(flavor = "multi_thread")]
async fn a_pipe_gets_a_table_and_json_comes_when_asked() {
    let server = server().await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.project/search_read"))
        .respond_with(ResponseTemplate::new(200).set_body_json(odoo_project()))
        .mount(&server)
        .await;

    // The default is for people, even into a pipe: aligned columns, no escape codes
    // because stdout is not a terminal here.
    hodoo(&server)
        .args(["project", "ls"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ID").and(predicate::str::contains("Acme Manufacturing")))
        .stdout(predicate::str::contains("6/8"))
        .stdout(predicate::str::contains("\u{1b}").not());

    // -o json is the contract scripts get.
    hodoo(&server)
        .args(["project", "ls", "-o", "json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"open_task_count\":6"))
        .stdout(predicate::str::contains(
            "\"privacy_visibility\":\"employees\"",
        ));

    // --json is the same thing, and HODOO_OUTPUT sets it once for a session.
    hodoo(&server)
        .args(["project", "ls", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("["));
    hodoo(&server)
        .env("HODOO_OUTPUT", "json")
        .args(["project", "ls"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("["));
}

#[tokio::test(flavor = "multi_thread")]
async fn headers_can_be_dropped_so_awk_and_grep_work() {
    let server = server().await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.project/search_read"))
        .respond_with(ResponseTemplate::new(200).set_body_json(odoo_project()))
        .mount(&server)
        .await;

    let output = hodoo(&server)
        .args(["project", "ls", "--no-headers"])
        .output()
        .expect("ran");
    let text = String::from_utf8(output.stdout).expect("utf8");
    assert!(!text.contains("ID"), "headers were dropped: {text}");
    assert_eq!(text.lines().count(), 1, "one row per record: {text}");
    // The first field is the id, so `awk '{print $1}'` works.
    assert!(text.starts_with("49"), "the row starts with the id: {text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_deadline_is_read_as_a_person_reads_it() {
    let server = server().await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/search_read"))
        .respond_with(ResponseTemplate::new(200).set_body_json(odoo_task(3)))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.task.type/read"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([{ "id": 11, "name": "Backlog" }])),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/json/2/res.users/read"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([{ "id": 5, "name": "odoo-agent" }])),
        )
        .mount(&server)
        .await;

    hodoo(&server)
        .args(["task", "ls", "--project", "49"])
        .assert()
        .success()
        .stdout(predicate::str::contains("in 3d"))
        .stdout(predicate::str::contains("high"))
        .stdout(predicate::str::contains("Backlog"))
        .stdout(predicate::str::contains("odoo-agent"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_change_reports_its_result_to_a_script_and_not_to_a_person() {
    let server = server().await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/create"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([77])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.project/search_read"))
        .respond_with(ResponseTemplate::new(200).set_body_json(odoo_project()))
        .mount(&server)
        .await;

    // `id=$(hodoo task create … | jq .id)` is the contract in JSON mode.
    hodoo(&server)
        .args([
            "task",
            "create",
            "--name",
            "Write the copy",
            "--project",
            "49",
            "-o",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("{\"id\":77}"))
        .stderr(predicate::str::is_empty());

    // A person gets a sentence on stderr and no payload on stdout.
    hodoo(&server)
        .args([
            "task",
            "create",
            "--name",
            "Write the copy",
            "--project",
            "49",
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("created  task #77"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dry_run_shows_the_change_and_sends_nothing() {
    let server = server().await;
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let recorder = {
        let calls = Arc::clone(&calls);
        move |request: &Request| {
            calls
                .lock()
                .expect("lock")
                .push(request.url.path().to_owned());
            ResponseTemplate::new(200).set_body_json(json!([99]))
        }
    };
    Mock::given(method("POST"))
        .respond_with(recorder)
        .mount(&server)
        .await;

    // The project has to be resolved first, which is a read: only writes stay unsent.
    hodoo(&server)
        .args([
            "task",
            "create",
            "--name",
            "Write the copy",
            "--project",
            "49",
            "--priority",
            "urgent",
            "-n",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("would create a task"))
        .stdout(predicate::str::contains("nothing was sent"))
        .stdout(predicate::str::contains("urgent"));

    let sent = calls.lock().expect("lock").clone();
    assert!(
        !sent.iter().any(|url| url.contains("/create")),
        "a dry run must not create: {sent:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn deleting_asks_first_and_a_script_must_force_it() {
    let server = server().await;
    let deleted = Arc::new(Mutex::new(false));
    let recorder = {
        let deleted = Arc::clone(&deleted);
        move |request: &Request| {
            if request.url.path().ends_with("/unlink") {
                *deleted.lock().expect("lock") = true;
                ResponseTemplate::new(200).set_body_json(json!(true))
            } else {
                ResponseTemplate::new(200).set_body_json(odoo_project())
            }
        }
    };
    Mock::given(method("POST"))
        .respond_with(recorder)
        .mount(&server)
        .await;

    // stdin is not a terminal here, so there is nobody to ask: refuse, exit 2, delete
    // nothing.
    hodoo(&server)
        .args(["project", "rm", "49"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("refusing to delete project #49"))
        .stderr(predicate::str::contains("-f/--force"));
    assert!(!*deleted.lock().expect("lock"), "nothing was deleted");

    // --no-input says the same thing even when someone is watching.
    hodoo(&server)
        .args(["project", "rm", "49", "--no-input"])
        .assert()
        .code(2);
    assert!(!*deleted.lock().expect("lock"), "still nothing deleted");

    // -f is how a script means it.
    hodoo(&server)
        .args(["project", "rm", "49", "-f"])
        .assert()
        .success()
        .stderr(predicate::str::contains("deleted  project #49"));
    assert!(*deleted.lock().expect("lock"), "the delete went through");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_odoo_error_is_one_sentence_on_stderr_and_exit_one() {
    let server = server().await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.project/search_read"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({
            "name": "werkzeug.exceptions.Unauthorized",
            "message": "Invalid apikey",
            "arguments": ["Invalid apikey", 401],
            "context": {},
            "debug": "Traceback (most recent call last): ..."
        })))
        .mount(&server)
        .await;

    hodoo(&server)
        .args(["project", "ls"])
        .assert()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "hodoo: Odoo error 401: Invalid apikey",
        ))
        .stderr(predicate::str::contains("Traceback").not());

    // -v adds the server's own traceback, and -o json turns it into data.
    hodoo(&server)
        .args(["project", "ls", "-v"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("Traceback"));
    hodoo(&server)
        .args(["project", "ls", "-o", "json"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("\"status\":401"))
        .stderr(predicate::str::contains("\"kind\":\"odoo\""));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_credential_says_what_to_do_about_it() {
    let mut command = Command::cargo_bin("hodoo").expect("the binary");
    command
        .current_dir(std::env::temp_dir())
        .env_remove("ODOO_URL")
        .env_remove("ODOO_API_KEY")
        .arg("project")
        .arg("ls")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no server to talk to"))
        .stderr(predicate::str::contains("--url"));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unknown_reference_lists_what_exists_instead_of_guessing() {
    let server = server().await;
    // One responder decides by the name asked for: two matches for "acme", none for
    // anything else.
    let responder = |request: &Request| {
        let asked = request
            .body_json::<Value>()
            .ok()
            .and_then(|body| body["domain"][0][2].as_str().map(str::to_owned))
            .unwrap_or_default();
        if asked.contains("acme") {
            ResponseTemplate::new(200).set_body_json(json!([
                { "id": 49, "name": "Acme Manufacturing - Website" },
                { "id": 50, "name": "Acme Manufacturing - Intranet" }
            ]))
        } else {
            ResponseTemplate::new(200).set_body_json(json!([]))
        }
    };
    Mock::given(method("POST"))
        .and(path("/json/2/project.project/search_read"))
        .respond_with(responder)
        .mount(&server)
        .await;

    // Two matches is not a choice to make silently.
    hodoo(&server)
        .args(["project", "show", "acme"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("matches 2 projects"))
        .stderr(predicate::str::contains("#49 Acme Manufacturing - Website"));

    // Nothing at all is a suggestion, not a stack trace.
    hodoo(&server)
        .args(["project", "show", "nothing-like-this"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no project matches"))
        .stderr(predicate::str::contains("hodoo project ls"));
}

#[tokio::test(flavor = "multi_thread")]
async fn call_is_the_escape_hatch_and_reads_well() {
    let server = server().await;
    Mock::given(method("POST"))
        .and(path("/json/2/res.partner/search_read"))
        .and(body_partial_json(
            json!({ "domain": [], "fields": ["name"] }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            { "id": 3, "name": "Acme Manufacturing" },
            { "id": 4, "name": "Nordwind Studio" }
        ])))
        .mount(&server)
        .await;

    // A list of records becomes a table for a person...
    hodoo(&server)
        .args([
            "call",
            "res.partner",
            "search_read",
            "--body",
            "{\"domain\":[],\"fields\":[\"name\"]}",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("id"))
        .stdout(predicate::str::contains("Nordwind Studio"));
    // ...and JSON for a script.
    hodoo(&server)
        .args([
            "call",
            "res.partner",
            "search_read",
            "--body",
            "{\"domain\":[],\"fields\":[\"name\"]}",
            "-o",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("[{\"id\":3"));

    // A body that is not an object is the caller's mistake, caught before any call.
    hodoo(&server)
        .args(["call", "res.partner", "search_read", "--body", "[1,2]"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("must be a JSON object"));
    hodoo(&server)
        .args(["call", "res.partner", "search_read", "--body", "{oops"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("not valid JSON"));
}

#[tokio::test(flavor = "multi_thread")]
async fn completions_and_help_need_no_server_at_all() {
    Command::cargo_bin("hodoo")
        .expect("the binary")
        .args(["completions", "bash"])
        .env_remove("ODOO_URL")
        .assert()
        .success()
        .stdout(predicate::str::contains("complete -F"));

    Command::cargo_bin("hodoo")
        .expect("the binary")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Getting started"))
        .stdout(predicate::str::contains("hodoo task create --name"));

    // No arguments at all explains itself on stderr, and says so with exit 2.
    Command::cargo_bin("hodoo")
        .expect("the binary")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Usage:"))
        .stderr(predicate::str::contains("hodoo task create --name"));

    // A typo is guessed at by clap.
    Command::cargo_bin("hodoo")
        .expect("the binary")
        .args(["projct", "ls"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("project"));

    // `board` answers differently per mode, which a script has to know before it
    // counts lines of JSON: the help says so.
    Command::cargo_bin("hodoo")
        .expect("the binary")
        .args(["board", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("counts tasks rather than"))
        .stdout(predicate::str::contains("lines"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_state_write_says_so_when_odoo_recomputes_it() {
    let server = server().await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/read"))
        .respond_with(ResponseTemplate::new(200).set_body_json(odoo_waiting_task()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/write"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(true)))
        .mount(&server)
        .await;

    // Odoo computes the state from open dependencies, so the write reads back as
    // waiting. A person gets a sentence where it happens...
    hodoo(&server)
        .args(["task", "update", "31", "--state", "changes-requested"])
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "reads waiting rather than changes requested",
        ))
        .stderr(predicate::str::contains("hodoo task deps 31"));

    // ...and a script gets its contract untouched: the hint never reaches JSON mode.
    hodoo(&server)
        .args([
            "task",
            "update",
            "31",
            "--state",
            "changes-requested",
            "-o",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("{\"id\":31,\"ok\":true}"))
        .stderr(predicate::str::is_empty());

    // A state that does stick says nothing extra.
    hodoo(&server)
        .args(["task", "update", "31", "--state", "changes-requested", "-q"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_delete_prompt_names_what_goes_with_it() {
    let server = server().await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.project/read"))
        .respond_with(ResponseTemplate::new(200).set_body_json(odoo_project()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/read"))
        .respond_with(ResponseTemplate::new(200).set_body_json(odoo_task(3)))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.milestone/search_count"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(3)))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/json/2/project.task/search_count"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!(2)))
        .mount(&server)
        .await;

    // Milestones cascade with the project, so the refusal says so before anyone
    // types -f and finds out afterwards.
    hodoo(&server)
        .args(["project", "rm", "49"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("refusing to delete project #49"))
        .stderr(predicate::str::contains("and its 8 tasks and 3 milestones"));

    // Subtasks go with their parent task.
    hodoo(&server)
        .args(["task", "rm", "31"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("and its 2 subtasks"));

    // --dry-run promises the same thing without asking. A delete's dry run is a note
    // rather than a preview, so it lands on stderr and stdout stays for the result.
    hodoo(&server)
        .args(["project", "rm", "49", "-n"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains(
            "would delete project #49 \"Acme Manufacturing - Website\" and its 8 tasks and 3 milestones",
        ));
}
