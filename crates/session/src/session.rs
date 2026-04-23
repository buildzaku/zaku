use gpui::{App, Subscription, Task, WindowId, prelude::*};

#[cfg(not(any(test, feature = "test-support")))]
use std::time::Duration;

use db::kv::KeyValueStore;
use util::ResultExt;

const SESSION_ID_KEY: &str = "session_id";
const SESSION_WINDOW_STACK_KEY: &str = "session_window_stack";

pub struct Session {
    session_id: String,
    old_session_id: Option<String>,
    old_window_ids: Option<Vec<WindowId>>,
}

impl Session {
    pub async fn new(session_id: String, kv_store: KeyValueStore) -> Self {
        let old_session_id = kv_store.read_kv(SESSION_ID_KEY).ok().flatten();

        kv_store
            .write_kv(SESSION_ID_KEY.to_string(), session_id.clone())
            .await
            .log_err();

        Self {
            session_id,
            old_session_id,
            old_window_ids: load_window_stack(&kv_store),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn test_new() -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            old_session_id: None,
            old_window_ids: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.session_id
    }
}

pub struct AppSession {
    session: Session,
    _serialization_task: Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl AppSession {
    pub fn new(session: Session, cx: &Context<Self>) -> Self {
        let _subscriptions = vec![cx.on_app_quit(Self::before_quit)];

        #[cfg(not(any(test, feature = "test-support")))]
        let _serialization_task = {
            let kv_store = KeyValueStore::global(cx);
            cx.spawn(async move |_, cx| {
                let mut current_window_stack = Vec::new();
                loop {
                    if let Some(windows) = cx.update(|cx| window_stack(cx))
                        && windows != current_window_stack
                    {
                        save_window_stack(kv_store.clone(), &windows).await;
                        current_window_stack = windows;
                    }

                    cx.background_executor()
                        .timer(Duration::from_millis(500))
                        .await;
                }
            })
        };

        #[cfg(any(test, feature = "test-support"))]
        let _serialization_task = Task::ready(());

        Self {
            session,
            _serialization_task,
            _subscriptions,
        }
    }

    fn before_quit(&mut self, cx: &mut Context<Self>) -> Task<()> {
        if let Some(window_stack) = window_stack(cx) {
            let kv_store = KeyValueStore::global(cx);
            cx.background_spawn(async move { save_window_stack(kv_store, &window_stack).await })
        } else {
            Task::ready(())
        }
    }

    pub fn id(&self) -> &str {
        self.session.id()
    }

    pub fn last_session_id(&self) -> Option<&str> {
        self.session.old_session_id.as_deref()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn replace_session(&mut self, session: Session) {
        self.session = session;
    }

    pub fn last_session_window_stack(&self) -> Option<Vec<WindowId>> {
        self.session.old_window_ids.clone()
    }
}

fn window_stack(cx: &App) -> Option<Vec<u64>> {
    Some(
        cx.window_stack()?
            .into_iter()
            .map(|window| window.window_id().as_u64())
            .collect(),
    )
}

pub fn load_window_stack(kv_store: &KeyValueStore) -> Option<Vec<WindowId>> {
    kv_store
        .read_kv(SESSION_WINDOW_STACK_KEY)
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str::<Vec<u64>>(&json).ok())
        .map(|window_ids| window_ids.into_iter().map(WindowId::from).collect())
}

pub async fn save_window_stack(kv_store: KeyValueStore, windows: &[u64]) {
    if let Ok(window_ids_json) = serde_json::to_string(windows) {
        kv_store
            .write_kv(SESSION_WINDOW_STACK_KEY.to_string(), window_ids_json)
            .await
            .log_err();
    }
}
