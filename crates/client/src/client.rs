pub mod telemetry;

#[cfg(any(test, feature = "test"))]
use clock::FakeSystemClock;
use clock::RealSystemClock;
use gpui::{App, Global};
use std::sync::Arc;

use http_client::{HttpClient, HttpClientWithUrl};

use crate::telemetry::Telemetry;

struct GlobalClient(Arc<Client>);

impl Global for GlobalClient {}

pub struct Client {
    http: Arc<HttpClientWithUrl>,
    telemetry: Arc<Telemetry>,
}

impl Client {
    pub fn new(http: Arc<dyn HttpClient>, cx: &mut App) -> Arc<Self> {
        let http = Arc::new(HttpClientWithUrl::new(http, metadata::ZAKU_SERVER_URL));

        Arc::new(Self {
            telemetry: Telemetry::new(Arc::new(RealSystemClock), http.clone(), cx),
            http,
        })
    }

    #[cfg(any(test, feature = "test"))]
    pub fn test_new(http: Arc<HttpClientWithUrl>, cx: &mut App) -> Arc<Self> {
        Arc::new(Self {
            telemetry: Telemetry::new(Arc::new(FakeSystemClock::new()), http.clone(), cx),
            http,
        })
    }

    pub fn http_client(&self) -> Arc<HttpClientWithUrl> {
        self.http.clone()
    }

    pub fn global(cx: &App) -> Arc<Self> {
        cx.global::<GlobalClient>().0.clone()
    }

    pub fn set_global(client: Arc<Client>, cx: &mut App) {
        cx.set_global(GlobalClient(client));
    }

    pub fn telemetry(&self) -> &Arc<Telemetry> {
        &self.telemetry
    }
}
