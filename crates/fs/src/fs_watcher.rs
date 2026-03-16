use anyhow::anyhow;
use notify::{Event, EventKind, RecursiveMode, Watcher as NotifyWatcher};
use parking_lot::Mutex;
use smol::channel::Sender;

#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::ops::Bound;

use std::{
    collections::{BTreeMap, HashMap},
    ops::DerefMut,
    path::Path,
    sync::{Arc, OnceLock},
};

use util::{ResultExt, path::SanitizedPath};

use crate::{PathEvent, PathEventKind, Watcher};

pub struct FsWatcher {
    tx: Sender<()>,
    pending_path_events: Arc<Mutex<Vec<PathEvent>>>,
    registrations: Mutex<BTreeMap<Arc<Path>, WatcherRegistrationId>>,
}

impl FsWatcher {
    pub fn new(tx: Sender<()>, pending_path_events: Arc<Mutex<Vec<PathEvent>>>) -> Self {
        Self {
            tx,
            pending_path_events,
            registrations: Default::default(),
        }
    }
}

impl Drop for FsWatcher {
    fn drop(&mut self) {
        let mut registrations = BTreeMap::new();
        {
            let old = &mut self.registrations.lock();
            std::mem::swap(old.deref_mut(), &mut registrations);
        }

        let _ = global(|watcher| {
            for (_, registration) in registrations {
                watcher.remove(registration);
            }
        });
    }
}

impl Watcher for FsWatcher {
    fn add(&self, path: &Path) -> anyhow::Result<()> {
        log::trace!("Adding watch for {path:?}");

        let tx = self.tx.clone();
        let pending_paths = self.pending_path_events.clone();

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            if let Some((watched_path, _)) = self
                .registrations
                .lock()
                .range::<Path, _>((Bound::Unbounded, Bound::Included(path)))
                .next_back()
                && path.starts_with(watched_path.as_ref())
            {
                log::trace!("Skipping watch for {path:?}; covered by {watched_path:?}");
                return Ok(());
            }
        }

        #[cfg(target_os = "linux")]
        {
            if self.registrations.lock().contains_key(path) {
                log::trace!("Skipping watch for {path:?}; already watched");
                return Ok(());
            }
        }

        let root_path = SanitizedPath::new_arc(path);
        let path: Arc<Path> = path.into();

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let mode = RecursiveMode::Recursive;

        #[cfg(target_os = "linux")]
        let mode = RecursiveMode::NonRecursive;

        let registration_id = global({
            let path = path.clone();
            let root_path = root_path.clone();

            move |watcher| {
                watcher.add(path, mode, move |event: &notify::Event| {
                    log::trace!("Received watch event: {event:?}");

                    let kind = match event.kind {
                        EventKind::Create(_) => Some(PathEventKind::Created),
                        EventKind::Modify(_) => Some(PathEventKind::Changed),
                        EventKind::Remove(_) => Some(PathEventKind::Removed),
                        _ => None,
                    };

                    let mut path_events = event
                        .paths
                        .iter()
                        .filter_map(|event_path| {
                            let event_path = SanitizedPath::new(event_path);
                            event_path.starts_with(&root_path).then(|| PathEvent {
                                path: event_path.as_path().to_path_buf(),
                                kind,
                            })
                        })
                        .collect::<Vec<_>>();

                    if !path_events.is_empty() {
                        path_events.sort();
                        let mut pending_paths = pending_paths.lock();
                        if pending_paths.is_empty() {
                            tx.try_send(()).ok();
                        }
                        util::extend_sorted(
                            &mut *pending_paths,
                            path_events,
                            usize::MAX,
                            |a, b| a.path.cmp(&b.path),
                        );
                    }
                })
            }
        })??;

        self.registrations.lock().insert(path, registration_id);
        Ok(())
    }

    fn remove(&self, path: &Path) -> anyhow::Result<()> {
        log::trace!("Removing watch for {path:?}");

        let Some(registration) = self.registrations.lock().remove(path) else {
            return Ok(());
        };

        global(|watcher| watcher.remove(registration))
    }
}

#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct WatcherRegistrationId(u32);

struct WatcherRegistrationState {
    callback: Arc<dyn Fn(&Event) + Send + Sync>,
    path: Arc<Path>,
}

struct WatcherState {
    watchers: HashMap<WatcherRegistrationId, WatcherRegistrationState>,
    path_registrations: HashMap<Arc<Path>, u32>,
    last_registration: WatcherRegistrationId,
}

#[cfg(target_os = "linux")]
type PlatformWatcher = notify::INotifyWatcher;

#[cfg(target_os = "windows")]
type PlatformWatcher = notify::ReadDirectoryChangesWatcher;

#[cfg(target_os = "macos")]
type PlatformWatcher = notify::FsEventWatcher;

pub struct GlobalWatcher {
    state: Mutex<WatcherState>,
    // Never keep the state lock while holding the watcher lock. Calling
    // `watch()` can synchronously trigger events that need the state lock again.
    watcher: Mutex<PlatformWatcher>,
}

impl GlobalWatcher {
    #[must_use]
    fn add(
        &self,
        path: Arc<Path>,
        mode: notify::RecursiveMode,
        callback: impl Fn(&notify::Event) + Send + Sync + 'static,
    ) -> anyhow::Result<WatcherRegistrationId> {
        let mut state = self.state.lock();

        #[cfg(any(target_os = "windows", target_os = "macos"))]
        let path_already_covered = state.path_registrations.keys().any(|existing| {
            path.starts_with(existing.as_ref()) && path.as_ref() != existing.as_ref()
        });

        #[cfg(target_os = "linux")]
        let path_already_covered = false;

        if !path_already_covered && !state.path_registrations.contains_key(&path) {
            drop(state);
            self.watcher.lock().watch(&path, mode)?;
            state = self.state.lock();
        }

        let id = state.last_registration;
        state.last_registration = WatcherRegistrationId(id.0 + 1);

        let registration_state = WatcherRegistrationState {
            callback: Arc::new(callback),
            path: path.clone(),
        };
        state.watchers.insert(id, registration_state);
        *state.path_registrations.entry(path).or_insert(0) += 1;

        Ok(id)
    }

    pub fn remove(&self, id: WatcherRegistrationId) {
        let mut state = self.state.lock();
        let Some(registration_state) = state.watchers.remove(&id) else {
            return;
        };

        let Some(count) = state.path_registrations.get_mut(&registration_state.path) else {
            return;
        };

        *count -= 1;

        if *count == 0 {
            state.path_registrations.remove(&registration_state.path);

            drop(state);
            self.watcher
                .lock()
                .unwatch(&registration_state.path)
                .log_err();
        }
    }
}

static FS_WATCHER_INSTANCE: OnceLock<anyhow::Result<GlobalWatcher, notify::Error>> =
    OnceLock::new();

fn handle_event(event: Result<notify::Event, notify::Error>) {
    log::trace!("Handling watch event: {event:?}");

    let Some(event) = event
        .log_err()
        .filter(|event| !matches!(event.kind, EventKind::Access(_)))
    else {
        return;
    };

    global::<()>(move |watcher| {
        let callbacks = {
            let state = watcher.state.lock();
            state
                .watchers
                .values()
                .map(|registration| registration.callback.clone())
                .collect::<Vec<_>>()
        };

        for callback in callbacks {
            callback(&event);
        }
    })
    .log_err();
}

pub fn global<T>(callback: impl FnOnce(&GlobalWatcher) -> T) -> anyhow::Result<T> {
    let result = FS_WATCHER_INSTANCE.get_or_init(|| {
        notify::recommended_watcher(handle_event).map(|watcher| GlobalWatcher {
            state: Mutex::new(WatcherState {
                watchers: Default::default(),
                path_registrations: Default::default(),
                last_registration: Default::default(),
            }),
            watcher: Mutex::new(watcher),
        })
    });

    match result {
        Ok(watcher) => Ok(callback(watcher)),
        Err(error) => Err(anyhow!("{error}")),
    }
}
