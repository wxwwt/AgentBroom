use agentbroom_core::cleanup::CleanupAnalysis;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativePlan {
    pub plan_id: String,
    pub test_only: bool,
    pub runtime_version: String,
    pub storage_profile: String,
    pub analysis: CleanupAnalysis,
    pub native_ids: Vec<String>,
    pub root_ids: Vec<String>,
    pub retained_ids: Vec<String>,
    pub blocked_reasons: Vec<String>,
    pub warnings: Vec<String>,
    pub locked_ids: Vec<String>,
    pub executable: bool,
    pub expires_in_seconds: u64,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Confirmation {
    pub plan_id: String,
    pub selected_ids: Vec<String>,
    pub phrase: String,
    pub acknowledged: bool,
    #[serde(default)]
    pub kept: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Receipt {
    pub plan_id: String,
    pub selected_ids: Vec<String>,
    pub test_only: bool,
    pub status: String,
    pub removed: Vec<String>,
    pub remaining: Vec<String>,
    pub observed_removed_logical_bytes: u64,
    pub freed_bytes: Option<u64>,
    pub error: Option<String>,
    #[serde(default)]
    pub concurrent_changes: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub source_id: String,
    #[serde(default)]
    pub expected_ids: Vec<String>,
    pub native_notifications_verified: bool,
    pub native_deleted_ids: Vec<String>,
    pub files_verified: bool,
    pub state_verified: bool,
    #[serde(default)]
    pub related_verified: bool,
}

/// The phase is produced by the deletion owner, never inferred from an error-code list.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionFailure {
    pub code: String,
    pub message: String,
    pub execution_phase: ExecutionPhase,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionPhase {
    NotStarted,
    Unknown,
}
impl ExecutionFailure {
    pub fn new(error: agentbroom_core::InspectError, execution_phase: ExecutionPhase) -> Self {
        Self {
            code: error.code,
            message: error.message,
            execution_phase,
        }
    }
}
