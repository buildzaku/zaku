use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    request::models::{HttpReq, ReqCfg, ReqMeta},
    space::models::SpaceReference,
};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct AppStore {
    pub active_spaceref: Option<SpaceReference>,
    pub spacerefs: Vec<SpaceReference>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct ReqBuf {
    pub meta: ReqMeta,
    pub config: ReqCfg,
}

impl ReqBuf {
    pub fn from_req(req: &HttpReq) -> Self {
        let meta = ReqMeta {
            file_name: req.meta.file_name.clone(),
            display_name: req.meta.display_name.clone(),
            has_unsaved_changes: true,
        };

        let config = ReqCfg {
            method: req.config.method.clone(),
            url: req.config.url.clone(),
            headers: req.config.headers.clone(),
            parameters: req.config.parameters.clone(),
            content_type: req.config.content_type.clone(),
            body: req.config.body.clone(),
        };

        Self { meta, config }
    }
}

pub struct SpaceCookies;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Type, Default)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct AudioNotification {
    pub on_req_finish: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct NotificationSettings {
    pub audio: AudioNotification,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, Type)]
pub struct SpaceSettings {
    pub theme: Theme,
    pub notifications: NotificationSettings,
}
