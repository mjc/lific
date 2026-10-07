//! Pure behavior used by the native project Files manager.

use crate::db::models::{LinkedEntity, ProjectAttachment};

pub(crate) const PAGE_SIZE: i64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MimeFilter {
    All,
    Image,
    Video,
    Audio,
    Text,
    Pdf,
    Archive,
    Other,
}

impl MimeFilter {
    fn from_value(value: &str) -> Self {
        match value {
            "image" => Self::Image,
            "video" => Self::Video,
            "audio" => Self::Audio,
            "text" => Self::Text,
            "pdf" => Self::Pdf,
            "archive" => Self::Archive,
            _ => Self::Other,
        }
    }

    pub(crate) fn value(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Image => Some("image"),
            Self::Video => Some("video"),
            Self::Audio => Some("audio"),
            Self::Text => Some("text"),
            Self::Pdf => Some("pdf"),
            Self::Archive => Some("archive"),
            Self::Other => Some("other"),
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Image => "Images",
            Self::Video => "Video",
            Self::Audio => "Audio",
            Self::Text => "Text",
            Self::Pdf => "PDF",
            Self::Archive => "Archives",
            Self::Other => "Other",
        }
    }
}

pub(crate) fn mime_label(class: &str) -> &'static str {
    MimeFilter::from_value(class).label()
}

pub(crate) fn sweep_countdown(seconds: i64) -> String {
    if seconds <= 0 {
        return "swept on the next pass".into();
    }
    let hours = seconds / 3_600;
    if hours >= 24 {
        let days = hours / 24;
        let suffix = if days == 1 { "" } else { "s" };
        return format!("swept in {days} day{suffix}");
    }
    if hours >= 1 {
        return format!("swept in {hours}h");
    }
    let minutes = (seconds / 60).max(1);
    format!("swept in {minutes} min")
}

pub(crate) fn delete_confirm_message(reference_count: usize) -> String {
    if reference_count == 0 {
        return "Removes the file. It has no references.".into();
    }
    let suffix = if reference_count == 1 { "" } else { "s" };
    format!("Removes the file and its {reference_count} reference{suffix}.")
}

pub(crate) fn can_delete(
    uploader_id: Option<i64>,
    viewer_id: Option<i64>,
    is_admin: bool,
    can_edit: bool,
) -> bool {
    if is_admin || can_edit || viewer_id.is_none() {
        return true;
    }
    uploader_id.is_some() && uploader_id == viewer_id
}

pub(crate) fn entity_href(project_identifier: &str, entity: &LinkedEntity) -> Option<String> {
    if let Some(page_id) = entity.page_id {
        return Some(format!("/{project_identifier}/pages/{page_id}"));
    }
    if let Some(identifier) = entity.identifier.as_deref()
        && !identifier.is_empty()
    {
        return Some(format!("/{project_identifier}/issues/{identifier}"));
    }
    None
}

pub(crate) fn entity_chip_label(entity: &LinkedEntity) -> String {
    let base = entity.identifier.as_deref().unwrap_or("unlinked");
    if entity.entity_type == "comment" {
        return format!("{base} (comment)");
    }
    base.into()
}

pub(crate) fn uploader_options(items: &[ProjectAttachment]) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    for item in items {
        if let Some(uploader) = item.uploader.as_deref()
            && !uploader.is_empty()
        {
            names.insert(uploader);
        }
    }
    names.into_iter().map(str::to_owned).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweep_countdown_uses_hourly_sweeper_precision() {
        assert_eq!(sweep_countdown(0), "swept on the next pass");
        assert_eq!(sweep_countdown(3_599), "swept in 59 min");
        assert_eq!(sweep_countdown(3_600), "swept in 1h");
        assert_eq!(sweep_countdown(86_400), "swept in 1 day");
    }

    #[test]
    fn deletion_copy_explains_the_reference_blast_radius() {
        assert_eq!(
            delete_confirm_message(0),
            "Removes the file. It has no references."
        );
        assert_eq!(
            delete_confirm_message(1),
            "Removes the file and its 1 reference."
        );
        assert_eq!(
            delete_confirm_message(3),
            "Removes the file and its 3 references."
        );
    }

    #[test]
    fn delete_button_mirror_matches_server_and_fails_open_for_unknown_viewer() {
        assert!(can_delete(Some(8), None, false, false));
        assert!(can_delete(Some(2), Some(2), false, false));
        assert!(can_delete(None, Some(2), true, false));
        assert!(can_delete(None, Some(2), false, true));
        assert!(!can_delete(Some(8), Some(2), false, false));
    }

    #[test]
    fn mime_filter_labels_match_files_chips() {
        assert_eq!(mime_label("image"), "Images");
        assert_eq!(mime_label("archive"), "Archives");
        assert_eq!(mime_label("unknown"), "Other");
        assert_eq!(PAGE_SIZE, 50);
        assert_eq!(MimeFilter::All.value(), None);
        assert_eq!(MimeFilter::Image.value(), Some("image"));
        assert_eq!(MimeFilter::Archive.label(), "Archives");
    }

    #[test]
    fn linked_entity_routes_prefer_pages_and_comment_labels_are_explicit() {
        let page_comment = LinkedEntity {
            entity_type: "comment".into(),
            entity_id: 4,
            identifier: Some("ISS-2".into()),
            title: "A comment".into(),
            page_id: Some(31),
        };
        assert_eq!(
            entity_href("PRJ", &page_comment).as_deref(),
            Some("/PRJ/pages/31")
        );
        assert_eq!(entity_chip_label(&page_comment), "ISS-2 (comment)");
        let unlinked = LinkedEntity {
            entity_type: "issue".into(),
            entity_id: 5,
            identifier: None,
            title: "Unlinked".into(),
            page_id: None,
        };
        assert_eq!(entity_href("PRJ", &unlinked), None);
        assert_eq!(entity_chip_label(&unlinked), "unlinked");
        let empty_identifier = LinkedEntity {
            identifier: Some(String::new()),
            ..unlinked
        };
        assert_eq!(entity_href("PRJ", &empty_identifier), None);
    }

    #[test]
    fn uploader_options_are_unique_and_sorted() {
        let items = vec![
            attachment("zeta"),
            attachment("alpha"),
            attachment("alpha"),
            attachment_none(),
            attachment(""),
        ];
        assert_eq!(
            uploader_options(&items),
            vec!["alpha".to_owned(), "zeta".to_owned()]
        );
    }

    fn attachment(uploader: &str) -> ProjectAttachment {
        ProjectAttachment {
            id: 1,
            filename: "file.png".into(),
            mime: "image/png".into(),
            mime_class: "image".into(),
            size_bytes: 1,
            uploader_id: None,
            uploader: Some(uploader.into()),
            uploader_display_name: None,
            created_at: String::new(),
            entities: Vec::new(),
        }
    }

    fn attachment_none() -> ProjectAttachment {
        let mut item = attachment("unused");
        item.uploader = None;
        item
    }
}
