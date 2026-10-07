use topcoat::runtime::record;

#[record]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RowCount {
    pub table: String,
    pub count: usize,
}

#[record]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ImportedProject {
    pub id: i64,
    pub identifier: String,
    pub is_public: bool,
}

#[record]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ImportReport {
    pub project: String,
    pub rows: Vec<RowCount>,
    pub blobs: usize,
    pub external_references: Vec<String>,
    pub external_reference_count: usize,
}

#[record]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ImportResult {
    pub project: ImportedProject,
    pub report: ImportReport,
}

pub(crate) const MAX_UPLOAD_BYTES: usize =
    crate::project_archive::Limits::WEB.max_compressed as usize;
pub(crate) const MAX_EXPANDED_BYTES: usize =
    crate::project_archive::Limits::WEB.max_expanded as usize;
