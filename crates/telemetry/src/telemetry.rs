pub use serde_json;
pub use telemetry_events::FlexibleEvent as Event;

use futures::channel::mpsc;
use std::sync::OnceLock;

/// Create a telemetry event and send it to the telemetry queue.
///
/// Use the "Noun Verbed" convention for event names, such as "App Opened"
/// or "Minidump Uploaded".
///
/// ```
/// telemetry::event!("App Opened");
/// telemetry::event!("Minidump Uploaded", crashed_version = "26.0");
/// ```
#[macro_export]
macro_rules! event {
    ($name:expr) => {{
        let event = $crate::Event {
            event_type: $name.to_string(),
            event_properties: std::collections::HashMap::new(),
        };
        $crate::send_event(event);
    }};
    ($name:expr, $($key:ident $(= $value:expr)?),+ $(,)?) => {{
        let event = $crate::Event {
            event_type: $name.to_string(),
            event_properties: std::collections::HashMap::from([
                $(
                    (stringify!($key).to_string(),
                        $crate::serde_json::value::to_value(&$crate::serialize_property!($key $(= $value)?))
                            .unwrap_or($crate::serde_json::Value::Null)
                    ),
                )+
            ]),
        };
        $crate::send_event(event);
    }};
}

#[macro_export]
macro_rules! serialize_property {
    ($key:ident) => {
        $key
    };
    ($key:ident = $value:expr) => {
        $value
    };
}

pub fn send_event(event: Event) {
    if let Some(queue) = TELEMETRY_QUEUE.get()
        && queue.unbounded_send(event).is_err()
    {
        log::trace!("Failed to send telemetry event because the queue is closed");
    }
}

pub fn init(tx: mpsc::UnboundedSender<Event>) {
    if TELEMETRY_QUEUE.set(tx).is_err() {
        log::warn!("Telemetry queue is already initialized");
    }
}

static TELEMETRY_QUEUE: OnceLock<mpsc::UnboundedSender<Event>> = OnceLock::new();
