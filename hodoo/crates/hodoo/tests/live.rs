//! End-to-end checks against a real Odoo. Ignored by default: they create and
//! delete records.
//!
//! ```text
//! HODOO_LIVE=1 ODOO_URL=https://odoo.example.com ODOO_API_KEY=... \
//!   cargo test --test live -- --ignored --nocapture
//! ```

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use std::time::Duration;

use hodoo::{
    Client, Config, MilestoneFields, Priority, ProjectFields, StageFields, TaskFields, TaskFilter,
    TaskState, Visibility,
};

/// A client built from the environment, or `None` when the live suite was not
/// asked for. The suite must never touch a real database by accident.
fn live_client() -> Option<Client> {
    if std::env::var("HODOO_LIVE").as_deref() != Ok("1") {
        eprintln!("skipped: set HODOO_LIVE=1 to run the live suite");
        return None;
    }
    let file = hodoo::dotenv::load(env!("CARGO_MANIFEST_DIR"))
        .expect("a readable .env")
        .unwrap_or_default();
    let url = hodoo::dotenv::resolve("ODOO_URL", &file).expect("ODOO_URL is set or in a .env");
    let key = hodoo::dotenv::resolve("ODOO_API_KEY", &file).expect("ODOO_API_KEY");
    let mut config = Config::new(url, key)
        .expect("config")
        .with_timeout(Duration::from_secs(60));
    if hodoo::dotenv::resolve("HODOO_INSECURE", &file).as_deref() == Some("1") {
        config = config.accept_invalid_certs(true);
    }
    if let Some(db) = hodoo::dotenv::resolve("ODOO_DB", &file) {
        config = config.with_db(db);
    }
    Some(Client::new(config).expect("client"))
}

/// Things the test created, deleted afterwards even when an assertion fails.
struct Cleanup {
    client: Client,
    created: Vec<(String, i64)>,
}

impl Cleanup {
    fn new(client: &Client) -> Self {
        Self {
            client: client.clone(),
            created: Vec::new(),
        }
    }

    fn track(&mut self, model: &str, id: impl Into<i64>) {
        self.created.push((model.to_owned(), id.into()));
    }

    /// Deletes in reverse creation order, so children go before parents.
    async fn run(&self) {
        for (model, id) in self.created.iter().rev() {
            let result = self
                .client
                .call(model, "unlink", serde_json::json!({ "ids": [id] }))
                .await;
            if let Err(error) = result {
                eprintln!("cleanup: could not delete {model} {id}: {error}");
            }
        }
    }
}

/// A name unique to this run, so leftovers are identifiable and re-runs do not
/// collide.
fn stamp() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    format!("hodoo-live-{now}")
}

#[tokio::test]
#[ignore = "creates and deletes records on a live Odoo; needs HODOO_LIVE=1"]
async fn version_and_whoami_prove_the_key_and_the_certificate() {
    let Some(client) = live_client() else {
        return;
    };

    let version = client.version().await.expect("version");
    assert!(
        version.starts_with("19"),
        "this crate targets Odoo 19, got {version}"
    );

    let whoami = client.whoami().await.expect("whoami");
    eprintln!("acting as uid {:?}", whoami.get("uid"));
}

#[tokio::test]
#[ignore = "creates and deletes records on a live Odoo; needs HODOO_LIVE=1"]
async fn a_project_with_a_staged_task_commented_milestoned_and_searched() {
    let Some(client) = live_client() else {
        return;
    };
    let mut cleanup = Cleanup::new(&client);
    let name = stamp();

    let outcome: hodoo::Result<()> = async {
        // A project, and the task stages it must have before a task can be staged.
        let project = client
            .projects()
            .create(ProjectFields {
                description: Some("<p>created by the hodoo live suite</p>".into()),
                visibility: Some(Visibility::Employees),
                allow_milestones: Some(true),
                ..ProjectFields::new(&name)
            })
            .await?;
        cleanup.track("project.project", project.get());

        let backlog = client
            .stages()
            .create_in(project, StageFields::new("Backlog"))
            .await?;
        cleanup.track("project.task.type", backlog.get());
        let done = client
            .stages()
            .create_in(
                project,
                StageFields {
                    fold: Some(true),
                    ..StageFields::new("Shipped")
                },
            )
            .await?;
        cleanup.track("project.task.type", done.get());

        let created = client.projects().get(project).await?;
        assert!(
            created.task_stages.contains(&backlog),
            "the stage was attached"
        );
        assert!(created.task_stages.contains(&done), "the second stage too");
        assert_eq!(created.visibility, Visibility::Employees);

        // A task in the backlog stage, assigned, dated, prioritised, tagged.
        let tag = client.tags().ensure(&format!("{name}-tag")).await?;
        // Tags are global rather than owned by the project, so the cleanup list
        // has to carry this one too or every run leaves a tag behind.
        cleanup.track("project.tags", tag.get());
        let me = client.whoami().await?;
        let assignee = me
            .get("uid")
            .and_then(serde_json::Value::as_i64)
            .expect("a uid in the context");
        let due = hodoo::datetime::parse_datetime("2027-01-15 09:00:00")?;

        let task = client
            .tasks()
            .create(TaskFields {
                project: Some(project),
                stage: Some(backlog),
                priority: Some(Priority::High),
                assignees: Some(vec![hodoo::UserId::new(assignee)]),
                deadline: Some(due),
                allocated_hours: Some(3.0),
                tags: Some(vec![tag]),
                description: Some("<p>live suite task</p>".into()),
                ..TaskFields::new(format!("{name} task"))
            })
            .await?;
        cleanup.track("project.task", task.get());

        let read = client.tasks().get(task).await?;
        assert_eq!(read.project, Some(project));
        assert_eq!(read.stage, Some(backlog));
        assert_eq!(read.priority, Priority::High);
        assert_eq!(read.deadline, Some(due));
        assert_node(&read, "the assignee Odoo added", assignee);

        // Chatter, two ways. Odoo also posts its own tracking messages here,
        // so the assertion is on our two bodies, not on the thread's length.
        client
            .tasks()
            .comment(task, "a note for internal users", true)
            .await?;
        client
            .tasks()
            .comment(task, "a comment for followers", false)
            .await?;
        let messages = client.tasks().messages(task, Some(50)).await?;
        let bodies: Vec<String> = messages.iter().filter_map(|m| m.body.clone()).collect();
        assert!(
            bodies
                .iter()
                .any(|body| body.contains("a note for internal users")),
            "the internal note is on the thread: {bodies:?}"
        );
        assert!(
            bodies
                .iter()
                .any(|body| body.contains("a comment for followers")),
            "the comment is on the thread: {bodies:?}"
        );

        // A milestone, on a project created with milestones enabled.
        let milestone = client
            .milestones()
            .create(MilestoneFields::new(project, format!("{name} beta")))
            .await?;
        cleanup.track("project.milestone", milestone.get());
        client.milestones().set_reached(milestone, true).await?;
        let milestones = client.milestones().list(project).await?;
        assert!(milestones.iter().any(|m| m.id == milestone && m.is_reached));

        // A subtask, and the searches an agent would actually use.
        let child = client
            .tasks()
            .create(TaskFields {
                parent: Some(task),
                ..TaskFields::new(format!("{name} subtask"))
            })
            .await?;
        cleanup.track("project.task", child.get());

        let open = client
            .tasks()
            .search(TaskFilter {
                project: Some(project),
                open_only: true,
                ..TaskFilter::default()
            })
            .await?;
        assert_eq!(open.len(), 2, "the task and its subtask are open");

        let due_soon = client
            .tasks()
            .search(TaskFilter {
                project: Some(project),
                deadline_before: Some(hodoo::datetime::parse_datetime("2027-02-01")?),
                ..TaskFilter::default()
            })
            .await?;
        assert_eq!(due_soon.len(), 1);

        assert!(client.tasks().count(TaskFilter::default()).await? >= 2);

        // Closing the task takes it out of the open list.
        client.tasks().set_state(task, TaskState::Done).await?;
        let closed = client.tasks().get(task).await?;
        assert_eq!(closed.state, TaskState::Done);
        assert!(closed.is_closed);

        let open = client
            .tasks()
            .search(TaskFilter {
                project: Some(project),
                open_only: true,
                ..TaskFilter::default()
            })
            .await?;
        assert_eq!(open.len(), 1, "only the subtask is left open");

        Ok(())
    }
    .await;

    cleanup.run().await;
    outcome.expect("the live lifecycle");
}

fn assert_node(task: &hodoo::Task, what: &str, assignee: i64) {
    assert!(
        task.assignees.contains(&hodoo::UserId::new(assignee)),
        "{what} is in {:?}",
        task.assignees
    );
}
