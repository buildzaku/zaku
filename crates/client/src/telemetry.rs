use clock::SystemClock;
use futures::{StreamExt, channel::mpsc};
use gpui::{App, AppContext, BackgroundExecutor, Task};
use jiff::Timestamp;
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::{
    env,
    fmt::Write as _,
    fs::File,
    io::Write as _,
    mem,
    path::PathBuf,
    sync::{Arc, LazyLock},
    time::{Duration, Instant},
};

use app_version::{AppVersion, ReleaseChannel};
use http_client::{AsyncBody, HttpClient, HttpClientWithUrl, Method, Request, StatusCode};
#[cfg(not(any(test, feature = "test")))]
use telemetry_events::FlexibleEvent;
use telemetry_events::{Event, EventRequestBody, EventWrapper};

struct TelemetryState {
    system_id: Option<Arc<str>>,
    installation_id: Option<Arc<str>>,
    session_id: Option<String>,
    session_started_at_ms: i64,
    release_channel: ReleaseChannel,
    arch: &'static str,
    events_queue: Vec<EventWrapper>,
    scheduled_flush_task: Option<Task<()>>,
    log_file: Option<File>,
    first_event_at: Option<Instant>,
    max_queue_size: usize,
    os_name: String,
    app_version: AppVersion,
    os_version: Option<String>,
}

#[cfg(debug_assertions)]
const MAX_QUEUE_SIZE: usize = 5;

#[cfg(not(debug_assertions))]
const MAX_QUEUE_SIZE: usize = 50;

#[cfg(debug_assertions)]
const FLUSH_INTERVAL: Duration = Duration::from_secs(1);

#[cfg(not(debug_assertions))]
const FLUSH_INTERVAL: Duration = Duration::from_mins(5);

static ZAKU_CLIENT_CHECKSUM_SEED: LazyLock<Option<&'static [u8]>> =
    LazyLock::new(|| option_env!("ZAKU_CLIENT_CHECKSUM_SEED").map(str::as_bytes));

pub static MINIDUMP_ENDPOINT: LazyLock<Option<String>> = LazyLock::new(|| {
    option_env!("ZAKU_MINIDUMP_ENDPOINT")
        .map(str::to_string)
        .or_else(|| env::var("ZAKU_MINIDUMP_ENDPOINT").ok())
});

pub fn should_install_crash_handler(channel: ReleaseChannel) -> bool {
    env::var("ZAKU_GENERATE_MINIDUMPS").is_ok_and(|value| value == "1")
        || (channel != ReleaseChannel::Dev && MINIDUMP_ENDPOINT.is_some())
}

pub struct Telemetry {
    clock: Arc<dyn SystemClock>,
    http_client: Arc<HttpClientWithUrl>,
    executor: BackgroundExecutor,
    state: Arc<Mutex<TelemetryState>>,
}

impl Telemetry {
    pub fn new(
        clock: Arc<dyn SystemClock>,
        http_client: Arc<HttpClientWithUrl>,
        cx: &mut App,
    ) -> Arc<Self> {
        let app_version = metadata::version(cx);
        let release_channel = app_version
            .release_channel()
            .expect("application version should have a release channel");
        let state = Arc::new(Mutex::new(TelemetryState {
            system_id: None,
            installation_id: None,
            session_id: None,
            session_started_at_ms: Timestamp::now().as_millisecond(),
            release_channel,
            arch: env::consts::ARCH,
            events_queue: Vec::new(),
            scheduled_flush_task: None,
            log_file: None,
            first_event_at: None,
            max_queue_size: MAX_QUEUE_SIZE,
            os_name: system_specs::os_name(),
            app_version,
            os_version: None,
        }));

        #[cfg(not(any(test, feature = "test")))]
        cx.background_spawn({
            let state = state.clone();
            async move {
                let os_version = system_specs::os_version();
                match File::create(Self::log_file_path()) {
                    Ok(log_file) => {
                        let mut state = state.lock();
                        state.os_version = Some(os_version);
                        state.log_file = Some(log_file);
                    }
                    Err(error) => {
                        state.lock().os_version = Some(os_version);
                        log::error!("Failed to open telemetry log: {error}");
                    }
                }
            }
        })
        .detach();

        let this = Arc::new(Self {
            clock,
            http_client,
            executor: cx.background_executor().clone(),
            state,
        });

        let (tx, mut rx) = mpsc::unbounded();
        ::telemetry::init(tx);

        cx.background_spawn({
            let this = Arc::downgrade(&this);
            async move {
                if cfg!(any(test, feature = "test")) {
                    return;
                }

                while let Some(event) = rx.next().await {
                    let Some(this) = this.upgrade() else {
                        break;
                    };
                    this.report_event(Event::Flexible(event));
                }
            }
        })
        .detach();

        #[cfg(not(any(test, feature = "test")))]
        cx.on_app_quit({
            let this = this.clone();

            move |_| {
                this.report_event(Event::Flexible(FlexibleEvent {
                    event_type: "App Closed".to_string(),
                    event_properties: std::collections::HashMap::new(),
                }));
                this.flush_events()
            }
        })
        .detach();

        this
    }

    pub fn log_file_path() -> PathBuf {
        path::logs_dir().join("telemetry.log")
    }

    pub fn start(
        self: &Arc<Self>,
        system_id: Option<String>,
        installation_id: Option<String>,
        session_id: String,
    ) {
        let mut state = self.state.lock();
        state.system_id = system_id.map(Into::into);
        state.installation_id = installation_id.map(Into::into);
        state.session_id = Some(session_id);
    }

    fn report_event(self: &Arc<Self>, event: Event) {
        let mut state = self.state.lock();
        log::trace!(target: "telemetry", "{event:?}");

        if state.scheduled_flush_task.is_none() {
            let this = self.clone();
            state.scheduled_flush_task = Some(self.executor.spawn(async move {
                this.executor.timer(FLUSH_INTERVAL).await;
                this.flush_events().detach();
            }));
        }

        let now = self.clock.utc_now();
        let elapsed_ms = if let Some(first_event_at) = state.first_event_at {
            u32::try_from(
                now.saturating_duration_since(first_event_at)
                    .min(Duration::from_hours(24))
                    .as_millis(),
            )
            .expect("elapsed event time should fit in u32")
        } else {
            state.first_event_at = Some(now);
            0
        };

        state.events_queue.push(EventWrapper { elapsed_ms, event });

        if state.installation_id.is_some() && state.events_queue.len() >= state.max_queue_size {
            drop(state);
            self.flush_events().detach();
        }
    }

    pub fn system_id(self: &Arc<Self>) -> Option<Arc<str>> {
        self.state.lock().system_id.clone()
    }

    pub fn installation_id(self: &Arc<Self>) -> Option<Arc<str>> {
        self.state.lock().installation_id.clone()
    }

    fn build_request(
        self: &Arc<Self>,
        mut json_bytes: Vec<u8>,
        event_request: &EventRequestBody,
    ) -> anyhow::Result<Request<AsyncBody>> {
        json_bytes.clear();
        serde_json::to_writer(&mut json_bytes, event_request)?;

        let checksum = calculate_json_checksum(&json_bytes).unwrap_or_default();

        Ok(Request::builder()
            .method(Method::POST)
            .uri(self.http_client.build_url("/telemetry/events"))
            .header("Content-Type", "application/json")
            .header("x-zaku-checksum", checksum)
            .body(json_bytes.into())?)
    }

    pub async fn flush_events_inner(self: &Arc<Self>) -> anyhow::Result<()> {
        let (json_bytes, request_body) = {
            let mut state = self.state.lock();
            state.first_event_at = None;
            let events = mem::take(&mut state.events_queue);
            state.scheduled_flush_task.take();
            if events.is_empty() {
                return Ok(());
            }

            let mut json_bytes = Vec::new();
            if let Some(file) = &mut state.log_file {
                let log_result = events.iter().try_for_each(|event| -> anyhow::Result<()> {
                    json_bytes.clear();
                    serde_json::to_writer(&mut json_bytes, event)?;
                    file.write_all(&json_bytes)?;
                    file.write_all(b"\n")?;
                    Ok(())
                });
                if let Err(error) = log_result {
                    log::error!("Failed to write telemetry log: {error:#}");
                    state.log_file = None;
                }
            }

            (
                json_bytes,
                EventRequestBody {
                    system_id: state.system_id.as_deref().map(Into::into),
                    installation_id: state.installation_id.as_deref().map(Into::into),
                    session_id: state.session_id.clone(),
                    session_started_at_ms: state.session_started_at_ms,
                    app_version: state.app_version.to_string(),
                    os_name: state.os_name.clone(),
                    os_version: state.os_version.clone(),
                    arch: state.arch.to_string(),
                    release_channel: state.release_channel.to_string(),
                    events,
                },
            )
        };

        let request = self.build_request(json_bytes, &request_body)?;
        let response = self.http_client.send(request).await?;
        if response.status() != StatusCode::OK {
            log::error!(
                "Failed to send telemetry events: HTTP {:?}",
                response.status()
            );
        }

        Ok(())
    }

    pub fn flush_events(self: &Arc<Self>) -> Task<()> {
        let this = self.clone();
        self.executor.spawn(async move {
            if let Err(error) = this.flush_events_inner().await {
                log::error!("Failed to flush telemetry events: {error:#}");
            }
        })
    }
}

pub fn calculate_json_checksum(json: &impl AsRef<[u8]>) -> Option<String> {
    let checksum_seed = ZAKU_CLIENT_CHECKSUM_SEED.as_ref()?;

    let mut digest = Sha256::new();
    digest.update(checksum_seed);
    digest.update(json);
    digest.update(checksum_seed);
    let mut checksum = String::new();
    for byte in digest.finalize() {
        write!(&mut checksum, "{byte:02x}").expect("writing to a string should not fail");
    }

    Some(checksum)
}

#[cfg(test)]
mod tests {
    use super::*;

    use clock::FakeSystemClock;
    use gpui::TestAppContext;
    use std::collections::HashMap;

    use http_client::FakeHttpClient;
    use telemetry_events::FlexibleEvent;

    fn init_test(cx: &mut TestAppContext) {
        cx.update(|cx| {
            metadata::init_test(
                env!("CARGO_PKG_VERSION")
                    .parse()
                    .expect("invalid version in Cargo.toml"),
                cx,
            );
        });
    }

    fn is_empty_state(telemetry: &Telemetry) -> bool {
        let state = telemetry.state.lock();
        state.events_queue.is_empty()
            && state.scheduled_flush_task.is_none()
            && state.first_event_at.is_none()
    }

    #[gpui::test]
    fn test_telemetry_flush_on_max_queue_size(cx: &mut TestAppContext) {
        init_test(cx);
        let executor = cx.executor();
        let clock = Arc::new(FakeSystemClock::new());
        let http_client = FakeHttpClient::with_response(StatusCode::OK);
        let system_id = Some("system_id".to_string());
        let installation_id = Some("installation_id".to_string());
        let session_id = "session_id".to_string();

        let (telemetry, first_event_at, event) = cx.update(|cx| {
            let telemetry = Telemetry::new(clock.clone(), http_client, cx);
            telemetry.state.lock().max_queue_size = 4;
            telemetry.start(system_id, installation_id, session_id);

            assert!(is_empty_state(&telemetry));

            let first_event_at = clock.utc_now();
            let event = FlexibleEvent {
                event_type: "test".to_string(),
                event_properties: HashMap::from([(
                    "test_key".to_string(),
                    serde_json::Value::String("test_value".to_string()),
                )]),
            };

            (telemetry, first_event_at, event)
        });

        cx.update(|_| {
            telemetry.report_event(Event::Flexible(event.clone()));
            assert_eq!(telemetry.state.lock().events_queue.len(), 1);
            assert!(telemetry.state.lock().scheduled_flush_task.is_some());
            assert_eq!(telemetry.state.lock().first_event_at, Some(first_event_at));

            clock.advance(Duration::from_millis(100));

            telemetry.report_event(Event::Flexible(event.clone()));
            assert_eq!(telemetry.state.lock().events_queue.len(), 2);
            assert!(telemetry.state.lock().scheduled_flush_task.is_some());
            assert_eq!(telemetry.state.lock().first_event_at, Some(first_event_at));

            clock.advance(Duration::from_millis(100));

            telemetry.report_event(Event::Flexible(event.clone()));
            assert_eq!(telemetry.state.lock().events_queue.len(), 3);
            assert!(telemetry.state.lock().scheduled_flush_task.is_some());
            assert_eq!(telemetry.state.lock().first_event_at, Some(first_event_at));

            clock.advance(Duration::from_millis(100));
            telemetry.report_event(Event::Flexible(event));
        });

        executor.run_until_parked();

        cx.update(|_| {
            assert!(is_empty_state(&telemetry));
        });
    }

    #[gpui::test]
    fn test_telemetry_flush_on_flush_interval(cx: &mut TestAppContext) {
        init_test(cx);
        let executor = cx.executor();
        let clock = Arc::new(FakeSystemClock::new());
        let http_client = FakeHttpClient::with_response(StatusCode::OK);
        let system_id = Some("system_id".to_string());
        let installation_id = Some("installation_id".to_string());
        let session_id = "session_id".to_string();

        cx.update(|cx| {
            let telemetry = Telemetry::new(clock.clone(), http_client, cx);
            telemetry.state.lock().max_queue_size = 4;
            telemetry.start(system_id, installation_id, session_id);

            assert!(is_empty_state(&telemetry));
            let first_event_at = clock.utc_now();
            let event = FlexibleEvent {
                event_type: "test".to_string(),
                event_properties: HashMap::from([(
                    "test_key".to_string(),
                    serde_json::Value::String("test_value".to_string()),
                )]),
            };

            telemetry.report_event(Event::Flexible(event));
            assert_eq!(telemetry.state.lock().events_queue.len(), 1);
            assert!(telemetry.state.lock().scheduled_flush_task.is_some());
            assert_eq!(telemetry.state.lock().first_event_at, Some(first_event_at));

            let duration = Duration::from_millis(1);
            executor.advance_clock(
                FLUSH_INTERVAL
                    .checked_sub(duration)
                    .expect("flush interval should exceed test duration"),
            );
            assert!(!is_empty_state(&telemetry));

            executor.advance_clock(duration);
            assert!(is_empty_state(&telemetry));
        });
    }
}
