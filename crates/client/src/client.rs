pub mod telemetry;

#[cfg(any(test, feature = "test"))]
use clock::FakeSystemClock;
use clock::RealSystemClock;
use gpui::App;
use std::sync::Arc;

use http_client::{HttpClient, HttpClientWithUrl};
use settings::{RegisterSetting, Settings, SettingsContent};

use crate::telemetry::Telemetry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, RegisterSetting)]
pub struct TelemetrySettings {
    pub diagnostics: bool,
    pub metrics: bool,
}

impl Settings for TelemetrySettings {
    fn from_settings(content: &SettingsContent) -> Self {
        let telemetry = content.telemetry.as_ref();

        Self {
            diagnostics: telemetry
                .and_then(|telemetry| telemetry.diagnostics)
                .expect("telemetry diagnostics should be defaulted"),
            metrics: telemetry
                .and_then(|telemetry| telemetry.metrics)
                .expect("telemetry metrics should be defaulted"),
        }
    }
}

pub struct Client {
    http_client: Arc<HttpClientWithUrl>,
    telemetry: Arc<Telemetry>,
}

impl Client {
    pub fn new(http_client: Arc<dyn HttpClient>, cx: &mut App) -> Arc<Self> {
        let http_client = Arc::new(HttpClientWithUrl::new(
            http_client,
            metadata::ZAKU_SERVER_URL,
        ));

        Arc::new(Self {
            telemetry: Telemetry::new(Arc::new(RealSystemClock), http_client.clone(), cx),
            http_client,
        })
    }

    #[cfg(any(test, feature = "test"))]
    pub fn test_new(http_client: Arc<HttpClientWithUrl>, cx: &mut App) -> Arc<Self> {
        Arc::new(Self {
            telemetry: Telemetry::new(Arc::new(FakeSystemClock::new()), http_client.clone(), cx),
            http_client,
        })
    }

    pub fn http_client(&self) -> Arc<HttpClientWithUrl> {
        self.http_client.clone()
    }

    pub fn telemetry(&self) -> &Arc<Telemetry> {
        &self.telemetry
    }
}
