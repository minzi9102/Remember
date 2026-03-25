use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum SeriesStatus {
    Active,
    Silent,
    Archived,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesSummary {
    pub id: String,
    pub name: String,
    pub status: SeriesStatus,
    pub last_updated_at: String,
    pub latest_excerpt: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitItem {
    pub id: String,
    pub series_id: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesCreateData {
    pub series: SeriesSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesListData {
    pub items: Vec<SeriesSummary>,
    pub next_cursor: Option<String>,
    pub limit_echo: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitAppendData {
    pub commit: CommitItem,
    pub series: SeriesSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineListData {
    pub series_id: String,
    pub items: Vec<CommitItem>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesArchiveData {
    pub series_id: String,
    pub archived_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesScanSilentData {
    pub affected_series_ids: Vec<String>,
    pub threshold_days: u64,
}
