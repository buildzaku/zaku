use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRequestBody {
    /// Identifier unique to each system Zaku is installed on.
    pub system_id: Option<String>,
    /// Identifier unique to each Zaku installation.
    pub installation_id: Option<String>,
    /// Identifier unique to each application session.
    pub session_id: Option<String>,
    /// Application session start time in milliseconds since the Unix epoch.
    pub session_started_at_ms: i64,
    /// Application version.
    pub app_version: String,
    /// Operating system.
    pub os_name: String,
    /// Operating system version.
    pub os_version: Option<String>,
    /// Application architecture.
    pub arch: String,
    /// Application release channel.
    pub release_channel: String,
    /// Events included in the batch.
    pub events: Vec<EventWrapper>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventWrapper {
    /// Duration since the first event in the batch.
    pub elapsed_ms: u32,
    /// The event itself.
    #[serde(flatten)]
    pub event: Event,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    Flexible(FlexibleEvent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlexibleEvent {
    /// Name of the event.
    pub event_type: String,
    /// Properties associated with the event.
    pub event_properties: HashMap<String, serde_json::Value>,
}
