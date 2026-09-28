//! Schema drift: does every field this crate reads still exist in Odoo?
//!
//! Odoo's own dynamic documentation lists the fields of a model, so an Odoo
//! rename can be caught here instead of by a decode failure in production. The
//! key's user needs the Settings group: `/doc-bearer` is gated on
//! `api_doc.group_allow_doc`, which `base.group_system` implies.
//!
//! ```text
//! HODOO_LIVE=1 ODOO_URL=https://odoo.example.com ODOO_API_KEY=... \
//!   cargo test --test drift -- --ignored --nocapture
//! ```

#![cfg_attr(test, allow(clippy::expect_used, clippy::unwrap_used))]

use std::collections::BTreeSet;

use serde_json::Value;

/// The fields each module asks Odoo for, paired with the model they belong to.
fn expectations() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        ("project.project", hodoo::project::FIELDS),
        ("project.task", hodoo::task::FIELDS),
        ("project.task.type", hodoo::stage::FIELDS),
        ("project.milestone", hodoo::milestone::FIELDS),
        ("project.tags", hodoo::tag::FIELDS),
        ("mail.message", hodoo::chatter::FIELDS),
    ]
}

#[tokio::test]
#[ignore = "needs a live Odoo and a Settings-level key; set HODOO_LIVE=1"]
async fn every_field_this_crate_reads_still_exists_in_odoo() {
    if std::env::var("HODOO_LIVE").as_deref() != Ok("1") {
        eprintln!("skipped: set HODOO_LIVE=1 to run the drift suite");
        return;
    }
    let file = hodoo::dotenv::load(env!("CARGO_MANIFEST_DIR"))
        .expect("a readable .env")
        .unwrap_or_default();
    let url = hodoo::dotenv::resolve("ODOO_URL", &file).expect("ODOO_URL");
    let key = hodoo::dotenv::resolve("ODOO_API_KEY", &file).expect("ODOO_API_KEY");
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .danger_accept_invalid_certs(
            hodoo::dotenv::resolve("HODOO_INSECURE", &file).as_deref() == Some("1"),
        )
        .build()
        .expect("http client");

    let base = url.trim_end_matches('/');
    let mut problems = Vec::new();

    for (model, fields) in expectations() {
        let response = http
            .post(format!("{base}/doc-bearer/{model}.json"))
            .header(reqwest::header::AUTHORIZATION, format!("Bearer {key}"))
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body("{}")
            .send()
            .await
            .expect("the doc endpoint");
        let status = response.status();
        let body: Value = response.json().await.expect("the doc listing");
        if status.as_u16() == 403
            || body
                .get("name")
                .and_then(Value::as_str)
                .is_some_and(|name| name.ends_with("AccessError"))
        {
            eprintln!(
                "skipped: /doc-bearer needs a key whose user has the Settings group \
                 (api_doc.group_allow_doc): {}",
                body.get("message").unwrap_or(&body)
            );
            return;
        }
        assert!(
            status.is_success(),
            "{model}: the doc endpoint answered {status}: {body}"
        );

        let listed: BTreeSet<String> = body["fields"]
            .as_object()
            .unwrap_or_else(|| panic!("{model}: no fields in the listing: {body}"))
            .keys()
            .cloned()
            .collect();

        for field in fields {
            if !listed.contains(*field) {
                problems.push(format!(
                    "{model}.{field} is read by this crate but not offered by Odoo"
                ));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "schema drift:\n{}",
        problems.join("\n")
    );
}
