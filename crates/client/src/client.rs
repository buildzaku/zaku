pub mod telemetry;

#[cfg(any(test, feature = "test"))]
use clock::FakeSystemClock;
use clock::RealSystemClock;
use gpui::App;
use std::sync::Arc;

use http_client::{HttpClient, HttpClientWithUrl};

use crate::telemetry::Telemetry;

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
