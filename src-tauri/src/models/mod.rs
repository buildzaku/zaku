use serde::{Deserialize, Serialize};
use ts_rs::{ExportError, TS};

use collection::{Collection, CollectionMeta, CreateCollectionDto};
use request::{CreateRequestDto, Request, RequestConfig, RequestMeta};
use space::{CreateSpaceDto, Space, SpaceMeta, SpaceReference};
use zaku::{ZakuError, ZakuState};

pub mod collection;
pub mod request;
pub mod space;
pub mod zaku;

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct OpenDirectoryDialogOptions {
    pub title: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct DispatchNotificationOptions {
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct CreateNewCollection {
    pub parent_relative_path: String,
    pub relative_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct CreateNewRequest {
    pub parent_relative_path: String,
    pub relative_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../src/lib/bindings.ts")]
pub struct MoveTreeItemDto {
    pub source_relative_path: String,
    pub destination_relative_path: String,
}

pub fn generate_bindings() -> Result<(), ExportError> {
    CollectionMeta::export_all()?;
    Collection::export_all()?;
    CreateCollectionDto::export_all()?;

    OpenDirectoryDialogOptions::export_all()?;
    DispatchNotificationOptions::export_all()?;
    CreateNewCollection::export_all()?;
    CreateNewRequest::export_all()?;
    MoveTreeItemDto::export_all()?;

    RequestMeta::export_all()?;
    RequestConfig::export_all()?;
    Request::export_all()?;
    CreateRequestDto::export_all()?;

    CreateSpaceDto::export_all()?;
    SpaceMeta::export_all()?;
    Space::export_all()?;
    SpaceReference::export_all()?;

    ZakuState::export_all()?;
    ZakuError::export_all()?;

    return Ok(());
}
