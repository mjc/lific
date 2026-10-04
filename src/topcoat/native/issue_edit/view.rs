//! Reusable native issue controls. Display snapshots never authorize a save.

use topcoat::{context::Cx, view::BoxView};

use super::actions::Snapshot;

pub(crate) fn editor<'a>(cx: &'a Cx, snapshot: &Snapshot, can_edit: bool) -> BoxView<'a> {
    super::controls::editor(cx, snapshot, can_edit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::models::{Priority, Status},
        ratelimit::IpNetwork,
    };
    use std::{net::SocketAddr, sync::Arc};
    use topcoat::{context::CxTestBuilder, router::RemoteAddr, view::ViewExt};

    fn snapshot() -> Snapshot {
        Snapshot {
            identifier: "ACC-1".into(),
            seq: 23,
            title: "Saved title".into(),
            description: "Saved body".into(),
            status: Status::Active,
            priority: Priority::Medium,
            blocks: Vec::new(),
            blocked_by: Vec::new(),
            relates_to: vec!["ACC-2".into()],
            duplicates: Vec::new(),
            duplicated_by: Vec::new(),
        }
    }

    fn context(prefix: &str) -> Cx {
        let (mut parts, ()) = axum::http::Request::builder()
            .header("x-forwarded-prefix", prefix)
            .body(())
            .unwrap()
            .into_parts();
        parts
            .extensions
            .insert(RemoteAddr("127.0.0.1:3000".parse::<SocketAddr>().unwrap()));
        let proxies: Arc<[IpNetwork]> = vec![IpNetwork::parse("127.0.0.1").unwrap()].into();
        CxTestBuilder::new()
            .app_context(proxies)
            .request_context(parts)
            .build()
    }

    async fn render(cx: &Cx, snapshot: &Snapshot, can_edit: bool) -> String {
        editor(cx, snapshot, can_edit)
            .single()
            .await
            .unwrap()
            .render(cx)
    }

    #[tokio::test]
    async fn native_issue_edit_controls_render_text_and_original_metadata_choices() {
        let html = render(&Cx::default(), &snapshot(), true).await;
        for text in [
            "Saved title",
            "Saved body",
            "Add a description... (markdown supported)",
            "Backlog",
            "Todo",
            "Active",
            "Done",
            "Cancelled",
            "Urgent",
            "High",
            "Medium",
            "Low",
            "None",
        ] {
            assert!(
                html.contains(text),
                "missing original issue control: {text}"
            );
        }
        assert!(html.contains("<input"));
        assert!(html.contains("<textarea"));
        assert!(html.contains("/__native_issue_edit/save"));
        assert!(!html.contains("/api/"));
        assert!(!html.contains("<script"));
    }

    #[tokio::test]
    async fn native_issue_edit_controls_keep_viewers_read_only() {
        let html = render(&Cx::default(), &snapshot(), false).await;
        assert!(html.contains("<h1"));
        assert!(html.contains("Saved title"));
        assert!(html.contains("Saved body"));
        assert!(!html.contains("<input"));
        assert!(!html.contains("<textarea"));
        assert!(!html.contains("/__native_issue_edit/save"));
    }

    #[tokio::test]
    async fn native_issue_edit_controls_escape_hostile_title_and_body_as_data() {
        let mut snapshot = snapshot();
        snapshot.title = "--> --!><img src=\"x\" onerror=\"alert(1)\"> & title".into();
        snapshot.description = "--> --!></textarea><script>alert(1)</script> & body".into();
        for can_edit in [true, false] {
            let html = render(&Cx::default(), &snapshot, can_edit).await;
            // Signal declarations are inert protocol comments, not rendered elements.
            let mut visible_markup = String::new();
            let mut remaining = html.as_str();
            let mut hostile_signal_comments = 0;
            while let Some(start) = remaining.find("<!--::topcoat::") {
                visible_markup.push_str(&remaining[..start]);
                let comment = &remaining[start + 4..];
                let end = comment.find("-->").expect("complete framework comment");
                assert!(
                    !comment[..end].contains("--!>"),
                    "alternate comment terminator escaped"
                );
                let payload = &comment[..end];
                if payload.starts_with("::topcoat::signal(") {
                    assert!(
                        payload.ends_with(')'),
                        "complete signal declaration framing"
                    );
                    if payload.contains("<img src=") || payload.contains("<script") {
                        hostile_signal_comments += 1;
                        assert!(payload.contains("--&gt;"));
                        assert!(payload.contains("--!&gt;"));
                    }
                }
                remaining = &comment[end + 3..];
            }
            visible_markup.push_str(remaining);
            assert!(
                hostile_signal_comments > 0,
                "hostile signal payloads checked"
            );
            assert!(
                visible_markup.contains(
                    "--&gt; --!&gt;&lt;img src=\"x\" onerror=\"alert(1)\"&gt; &amp; title"
                )
            );
            assert!(visible_markup.contains(
                "--&gt; --!&gt;&lt;/textarea&gt;&lt;script&gt;alert(1)&lt;/script&gt; &amp; body"
            ));
            if can_edit {
                // Attribute values have a separate context: quotes and '&' are
                // escaped; '<' is inert inside the quoted value. Browser tests
                // assert that these exact values never create img/script nodes.
                assert!(visible_markup.contains(
                    "value=\"--> --!><img src=&quot;x&quot; onerror=&quot;alert(1)&quot;> &amp; title\""
                ));
            }
        }
    }

    #[tokio::test]
    async fn native_issue_edit_controls_mount_authorized_relation_links_once() {
        for prefix in ["", "/app", "/ACC"] {
            let html = render(&context(prefix), &snapshot(), true).await;
            assert!(html.contains(&format!("href=\"{prefix}/ACC/issues/ACC-2\"")));
            assert!(html.contains("ACC-2"));
        }
    }
}
