use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use time::format_description::well_known::Rfc3339;

use crate::{collection::models::Collection, store::SpaceSettings};

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct CreateSpaceDto {
    pub name: String,
    pub location: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct SpaceMeta {
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SpaceConfigFile {
    pub meta: SpaceMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct Space {
    pub abspath: String,
    pub meta: SpaceMeta,
    pub root_collection: Collection,
    pub cookies: HashMap<String, Vec<SpaceCookie>>,
    pub settings: SpaceSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SpaceReference {
    pub path: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SpaceCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: Option<String>,
    pub expires: Option<String>,
}

impl SpaceCookie {
    pub fn from_cookie_store(ck: &cookie_store::Cookie) -> Self {
        Self {
            name: ck.name().to_string(),
            value: ck.value().to_string(),
            domain: ck.domain().unwrap_or("").to_string(),
            path: ck.path().unwrap_or("/").to_string(),
            secure: ck.secure().unwrap_or(false),
            http_only: ck.http_only().unwrap_or(false),
            same_site: ck.same_site().map(|ss| format!("{ss:?}")),
            expires: match ck.expires() {
                Some(cookie::Expiration::DateTime(dt)) => Some(dt.format(&Rfc3339).unwrap()),
                _ => None,
            },
        }
    }

    pub fn from_raw_cookie(rc: &cookie::Cookie<'_>) -> Self {
        Self {
            name: rc.name().to_string(),
            value: rc.value().to_string(),
            domain: rc.domain().unwrap_or("").to_string(),
            path: rc.path().unwrap_or("/").to_string(),
            secure: rc.secure().unwrap_or(false),
            http_only: rc.http_only().unwrap_or(false),
            same_site: rc.same_site().map(|ss| format!("{ss:?}")),
            expires: match rc.expires() {
                Some(cookie::Expiration::DateTime(dt)) => Some(dt.format(&Rfc3339).unwrap()),
                _ => None,
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct RemoveCookieDto {
    pub domain: String,
    pub path: String,
    pub name: String,
}
