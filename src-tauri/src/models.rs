use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize)]
pub struct Overview {
    pub entries: u64,
    pub options: u64,
    pub linux_modules: u64,
    pub areas: u64,
    pub profiles: u64,
    pub schema_version: i64,
    pub database_path: String,
}

#[derive(Serialize)]
pub struct EntryIndex {
    pub id: String,
    pub title: String,
    pub module_name: String,
}

#[derive(Serialize)]
pub struct EntryDetail {
    pub id: String,
    pub area_name: String,
    pub module_name: String,
    pub payload: Value,
    pub area_id: String,
    pub module_id: String,
    pub revision: i64,
    pub favorite: bool,
    pub presentation: Presentation,
    pub blocks: Vec<BlockView>,
}

#[derive(Serialize, Debug)]
pub struct SearchResult {
    pub visual: bool,
    pub id: String,
    pub title: String,
    pub kind: String,
    pub area_id: String,
    pub area_name: String,
    pub module_id: String,
    pub module_name: String,
    pub snippet: String,
    pub match_kind: MatchKind,
    pub favorite: bool,
    pub learning_status: String,
}

#[derive(Serialize, PartialEq, Eq, Debug, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    Exact,
    Textual,
    Approximate,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchRequest {
    pub query: String,
    pub limit: Option<u32>,
    pub filters: Option<CatalogFilter>,
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(deny_unknown_fields)]
pub struct CatalogFilter {
    pub area_id: Option<String>,
    pub module_id: Option<String>,
    pub entry_type: Option<String>,
    pub learning_status: Option<String>,
    #[serde(default)]
    pub favorites_only: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowseRequest {
    pub filters: CatalogFilter,
    pub offset: u32,
}
#[derive(Serialize)]
pub struct CatalogPage {
    pub items: Vec<SearchResult>,
    pub total: u64,
}
#[derive(Serialize)]
pub struct Catalog {
    pub areas: Vec<Value>,
    pub modules: Vec<Value>,
    pub profiles: Vec<Value>,
    pub components: Vec<&'static str>,
}
#[derive(Serialize, Debug)]
pub struct Presentation {
    pub profile_id: String,
    pub source: String,
    pub components: Vec<String>,
}
#[derive(Serialize)]
pub struct BlockView {
    pub payload: Value,
    pub virtual_block: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresentationInput {
    pub module_id: String,
    pub expected_revision: i64,
    pub profile_id: Option<String>,
    pub enabled_components: Option<Vec<String>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewArea {
    pub id: String,
    pub name: String,
    pub profile_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewModule {
    pub id: String,
    pub name: String,
    pub area_id: String,
    pub parent_id: Option<String>,
    pub profile_id: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryInput {
    pub id: String,
    pub expected_revision: Option<i64>,
    pub patch: Value,
    #[serde(default)]
    pub confirm_personal: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockInput {
    pub entry_id: String,
    pub expected_revision: i64,
    pub patch: Value,
    #[serde(default)]
    pub confirm_personal: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockOrder {
    pub entry_id: String,
    pub expected_revision: i64,
    pub ids: Vec<String>,
}
#[derive(Serialize)]
pub struct History {
    pub visits: Vec<Value>,
    pub searches: Vec<Value>,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TermKind {
    Synonym,
    Personal,
}

impl TermKind {
    pub(crate) fn table(self) -> &'static str {
        match self {
            Self::Synonym => "synonyms",
            Self::Personal => "personal_vocabulary",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermInput {
    pub id: Option<i64>,
    pub kind: TermKind,
    pub phrase: String,
    pub target_entry_id: String,
}

#[derive(Serialize)]
pub struct VocabularyTerm {
    pub id: i64,
    pub kind: TermKind,
    pub phrase: String,
    pub target_entry_id: String,
    pub target_title: String,
}
