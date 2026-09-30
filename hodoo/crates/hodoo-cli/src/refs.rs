//! Turning what a person typed into what Odoo needs.
//!
//! Every place the CLI asks for a record accepts an **id or a name**: `--project 49`
//! and `--project acme` both work. Ids are exact and cheap; names are matched
//! exactly first, then case-insensitively as a substring, and an ambiguous or missing
//! name fails with the candidates listed, because guessing which "site" was meant is
//! how data ends up in the wrong project.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use hodoo::{
    Client, Error, Id, MilestoneId, PartnerId, ProjectFilter, ProjectId, ProjectStageFilter,
    ProjectStageId, StageFilter, TagId, TaskFilter, TaskId, TaskStageId, UserId,
};

/// A reference the user gave: either a bare id or a name to look up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ref {
    /// A numeric id, used as is.
    Id(i64),
    /// A name to resolve.
    Name(String),
}

impl Ref {
    /// Reads an argument as an id when it is all digits, else as a name.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text.trim().parse::<i64>() {
            Ok(id) if id > 0 => Ref::Id(id),
            _ => Ref::Name(text.trim().to_owned()),
        }
    }
}

/// Resolves a project reference.
///
/// # Errors
///
/// [`Error::Config`] with a suggestion when nothing matches, or when a name matches
/// several projects.
pub async fn project(client: &Client, reference: &Ref) -> hodoo::Result<ProjectId> {
    match reference {
        Ref::Id(id) => Ok(Id::new(*id)),
        Ref::Name(name) => {
            let found = client
                .projects()
                .search(ProjectFilter {
                    name_contains: Some(name.clone()),
                    limit: Some(10),
                    ..ProjectFilter::default()
                })
                .await?;
            let chosen = pick(
                name,
                "project",
                found
                    .iter()
                    .map(|project| (project.id.get(), project.name.clone())),
            )?;
            Ok(chosen)
        }
    }
}

/// Resolves a task reference.
///
/// # Errors
///
/// [`Error::Config`] with a suggestion when nothing matches, or when a name matches
/// several tasks.
pub async fn task(client: &Client, reference: &Ref) -> hodoo::Result<TaskId> {
    match reference {
        Ref::Id(id) => Ok(Id::new(*id)),
        Ref::Name(name) => {
            let found = client
                .tasks()
                .search(TaskFilter {
                    name_contains: Some(name.clone()),
                    limit: Some(10),
                    order: Some("id".to_owned()),
                    ..TaskFilter::default()
                })
                .await?;
            let chosen = pick(
                name,
                "task",
                found.iter().map(|task| (task.id.get(), task.name.clone())),
            )?;
            Ok(chosen)
        }
    }
}

/// Resolves a task stage, optionally scoped to a project: "Review" means the one
/// people see, not one of the twelve others.
///
/// # Errors
///
/// [`Error::Config`] when the name is unknown or ambiguous.
pub async fn stage(
    client: &Client,
    reference: &Ref,
    project: Option<ProjectId>,
) -> hodoo::Result<TaskStageId> {
    match reference {
        Ref::Id(id) => Ok(Id::new(*id)),
        Ref::Name(name) => {
            let found = client
                .stages()
                .list(StageFilter {
                    project,
                    limit: Some(50),
                    ..StageFilter::default()
                })
                .await?;
            let chosen = pick(
                name,
                "task stage",
                found
                    .iter()
                    .map(|stage| (stage.id.get(), stage.name.clone())),
            )?;
            Ok(chosen)
        }
    }
}

/// Resolves a `project.project.stage` reference, a *project's* stage.
///
/// # Errors
///
/// [`Error::Config`] when the name is unknown or ambiguous.
pub async fn project_stage(client: &Client, reference: &Ref) -> hodoo::Result<ProjectStageId> {
    match reference {
        Ref::Id(id) => Ok(Id::new(*id)),
        Ref::Name(name) => {
            // Filtered by name in Odoo, like every other resolver: a name matching
            // nothing then reads as "no match" instead of "matches all four", and a
            // partial name still finds its stage.
            let found = client
                .project_stages()
                .list(ProjectStageFilter {
                    name_contains: Some(name.clone()),
                    limit: Some(50),
                    ..ProjectStageFilter::default()
                })
                .await?;
            let chosen = pick(
                name,
                "project stage",
                found
                    .iter()
                    .map(|stage| (stage.id.get(), stage.name.clone())),
            )?;
            Ok(chosen)
        }
    }
}

/// Resolves a tag by name, creating it when it does not exist yet, so
/// `--tag urgent` works on a fresh database.
///
/// # Errors
///
/// Any error of the underlying calls.
pub async fn tag(client: &Client, name: &str) -> hodoo::Result<TagId> {
    client.tags().ensure(name.trim()).await
}

/// Resolves a milestone by name inside a project.
///
/// # Errors
///
/// [`Error::Config`] when nothing matches or the name is ambiguous.
pub async fn milestone(
    client: &Client,
    reference: &Ref,
    project: ProjectId,
) -> hodoo::Result<MilestoneId> {
    match reference {
        Ref::Id(id) => Ok(Id::new(*id)),
        Ref::Name(name) => {
            let found = client.milestones().list(project).await?;
            let chosen = pick(
                name,
                "milestone",
                found
                    .iter()
                    .map(|milestone| (milestone.id.get(), milestone.name.clone())),
            )?;
            Ok(chosen)
        }
    }
}

/// Resolves a user by id, login or name.
///
/// # Errors
///
/// [`Error::Config`] when nothing matches or the reference is ambiguous; the search
/// itself is `res.users`, reached through the escape hatch because the client does
/// not model users.
pub async fn user(client: &Client, reference: &Ref) -> hodoo::Result<UserId> {
    match reference {
        Ref::Id(id) => Ok(Id::new(*id)),
        Ref::Name(name) => {
            let rows = client
                .call(
                    "res.users",
                    "search_read",
                    serde_json::json!({
                        "domain": ["|", "|", ["login", "ilike", name], ["name", "ilike", name], ["name", "=", name]],
                        "fields": ["login", "name"],
                        "limit": 10,
                    }),
                )
                .await?;
            let candidates: Vec<(i64, String)> = rows
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .filter_map(|row| {
                            let id = row.get("id")?.as_i64()?;
                            let login = row.get("login").and_then(|v| v.as_str()).unwrap_or("");
                            let display = row.get("name").and_then(|v| v.as_str()).unwrap_or(login);
                            Some((id, format!("{display} ({login})")))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(pick(name, "user", candidates.into_iter())?)
        }
    }
}

/// Resolves a partner by name, for `--customer`.
///
/// # Errors
///
/// [`Error::Config`] when nothing matches or the name is ambiguous.
pub async fn partner(client: &Client, reference: &Ref) -> hodoo::Result<PartnerId> {
    match reference {
        Ref::Id(id) => Ok(Id::new(*id)),
        Ref::Name(name) => {
            let rows = client
                .call(
                    "res.partner",
                    "search_read",
                    serde_json::json!({
                        "domain": [["name", "ilike", name]],
                        "fields": ["name"],
                        "limit": 10,
                        "order": "id",
                    }),
                )
                .await?;
            let candidates: Vec<(i64, String)> = rows
                .as_array()
                .map(|rows| {
                    rows.iter()
                        .filter_map(|row| {
                            Some((
                                row.get("id")?.as_i64()?,
                                row.get("name")?.as_str()?.to_owned(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Ok(pick(name, "contact", candidates.into_iter())?)
        }
    }
}

/// Picks the one candidate a name refers to.
///
/// Exact match wins; a substring match wins if it is the only one; several
/// candidates are an error that lists them, because a guess here writes to the
/// wrong record.
fn pick<T>(
    wanted: &str,
    noun: &str,
    candidates: impl Iterator<Item = (i64, String)>,
) -> hodoo::Result<Id<T>> {
    let wanted_lower = wanted.to_lowercase();
    let all: Vec<(i64, String)> = candidates.collect();

    if let Some((id, _)) = all
        .iter()
        .find(|(_, name)| name.to_lowercase() == wanted_lower)
        .or_else(|| (all.len() == 1).then(|| &all[0]))
    {
        return Ok(Id::new(*id));
    }

    if all.is_empty() {
        return Err(Error::Config {
            message: format!(
                "no {noun} matches {wanted:?}. List what exists with `{}`",
                match noun {
                    "task" => "hodoo task ls".to_owned(),
                    "project" => "hodoo project ls".to_owned(),
                    "task stage" => "hodoo project task-stages ls <project>".to_owned(),
                    "project stage" => "hodoo project stages ls".to_owned(),
                    "milestone" => "hodoo milestone ls --project <project>".to_owned(),
                    "contact" => "hodoo call res.partner search_read --body '{}'".to_owned(),
                    _ => "hodoo call res.users search_read --body '{}'".to_owned(),
                }
            ),
        });
    }

    let listed = all
        .iter()
        .take(5)
        .map(|(id, name)| format!("#{id} {name}"))
        .collect::<Vec<_>>()
        .join(", ");
    Err(Error::Config {
        message: format!(
            "{wanted:?} matches {} {noun}s: {listed}. Use one of those ids",
            all.len()
        ),
    })
}

/// Parses what a person writes for a date or deadline: `today`, `tomorrow`,
/// `yesterday`, `+3d`, `+2w`, `2026-12-01`, `2026-12-01 09:00`, or an ISO stamp.
///
/// # Errors
///
/// [`Error::Config`] listing the accepted forms.
pub fn when(text: &str) -> hodoo::Result<DateTime<Utc>> {
    let text = text.trim();
    let today = Utc::now();
    let named = |offset: i64| {
        (today + Duration::days(offset))
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap_or_default()
            .and_utc()
    };
    let span = |unit: Option<char>, amount: i64| match unit {
        Some('d') | None => Duration::days(amount),
        Some('w') => Duration::weeks(amount),
        Some('h') => Duration::hours(amount),
        // A stray unit letter reads as days, the default, rather than refusing
        // the deadline over a typo in what is usually `+3`.
        Some(_) => Duration::days(amount),
    };

    if let Some(rest) = text.strip_prefix('+') {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(amount) = digits.parse::<i64>() {
            let unit = rest.chars().nth(digits.len());
            return Ok(today + span(unit, amount));
        }
    }
    match text.to_ascii_lowercase().as_str() {
        "today" | "now" => return Ok(today),
        "tomorrow" => return Ok(named(1)),
        "yesterday" => return Ok(named(-1)),
        _ => {}
    }
    if let Ok(stamp) = hodoo::datetime::parse_datetime(text) {
        return Ok(stamp);
    }
    Err(Error::Config {
        message: format!(
            "{text:?} is not a date. Try today, tomorrow, +3d, +2w, 2026-12-01 or \
             \"2026-12-01 09:00\""
        ),
    })
}

/// A date without a time, for fields Odoo stores as dates.
///
/// # Errors
///
/// [`Error::Config`] when the text is neither a date nor a datetime.
pub fn date(text: &str) -> hodoo::Result<NaiveDate> {
    hodoo::datetime::parse_date(text)
}
