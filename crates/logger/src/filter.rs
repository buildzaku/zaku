use log::{Level, LevelFilter};
use std::{
    collections::{HashMap, VecDeque},
    ops::Range,
    sync::{
        OnceLock, RwLock,
        atomic::{AtomicU8, Ordering},
    },
};

use crate::{SCOPE_DEPTH_MAX, SCOPE_STRING_SEP_STR, ScopeAlloc, ScopeRef, env_config::EnvFilter};

static ENV_FILTER: OnceLock<EnvFilter> = OnceLock::new();
static SCOPE_MAP: RwLock<ScopeMap> = RwLock::new(ScopeMap::empty());
static LEVEL_ENABLED_MAX_STATIC: AtomicU8 = AtomicU8::new(LEVEL_ENABLED_MAX_DEFAULT as u8);
static LEVEL_ENABLED_MAX_CONFIG: AtomicU8 = AtomicU8::new(LEVEL_ENABLED_MAX_DEFAULT as u8);

const LEVEL_ENABLED_MAX_DEFAULT: LevelFilter = LevelFilter::Info;
const DEFAULT_FILTERS: &[(&str, LevelFilter)] = &[
    #[cfg(target_os = "linux")]
    ("zbus", LevelFilter::Warn),
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    ("naga::back::spv::writer", LevelFilter::Warn),
    ("usvg::parser", LevelFilter::Error),
];

pub fn init_env_filter(filter: EnvFilter) {
    if let Some(level_max) = filter.level_global {
        LEVEL_ENABLED_MAX_STATIC.store(level_max as u8, Ordering::Release);
    }
    assert!(
        ENV_FILTER.set(filter).is_ok(),
        "Environment filter cannot be initialized twice"
    );
}

pub fn is_possibly_enabled_level(level: Level) -> bool {
    level as u8 <= LEVEL_ENABLED_MAX_CONFIG.load(Ordering::Acquire)
}

pub fn is_scope_enabled(scope: &ScopeRef<'_>, module_path: Option<&str>, level: Level) -> bool {
    if !is_possibly_enabled_level(level) {
        return false;
    }
    let is_enabled_by_default = level as u8 <= LEVEL_ENABLED_MAX_STATIC.load(Ordering::Acquire);
    let global_scope_map = SCOPE_MAP.read().unwrap_or_else(|error| {
        SCOPE_MAP.clear_poison();
        error.into_inner()
    });

    if global_scope_map.is_empty() {
        return is_enabled_by_default;
    }

    match global_scope_map.is_enabled(scope, module_path, level) {
        EnabledStatus::NotConfigured => is_enabled_by_default,
        EnabledStatus::Enabled => true,
        EnabledStatus::Disabled => false,
    }
}

pub fn refresh_from_settings<S: std::hash::BuildHasher>(settings: &HashMap<String, String, S>) {
    let env_config = ENV_FILTER.get();
    let map_new = ScopeMap::new_from_settings_and_env(settings, env_config, DEFAULT_FILTERS);
    let level_enabled_max = level_enabled_max(&map_new);
    LEVEL_ENABLED_MAX_CONFIG.store(level_enabled_max, Ordering::Release);

    {
        let mut global_map = SCOPE_MAP.write().unwrap_or_else(|error| {
            SCOPE_MAP.clear_poison();
            error.into_inner()
        });
        *global_map = map_new;
    }

    log::trace!("Log configuration updated");
}

fn level_enabled_max(scope_map: &ScopeMap) -> u8 {
    let mut level_enabled_max = LEVEL_ENABLED_MAX_STATIC.load(Ordering::Acquire);

    for entry in &scope_map.entries {
        if let Some(level) = entry.enabled {
            level_enabled_max = level_enabled_max.max(level as u8);
        }
    }

    for (_, level) in &scope_map.modules {
        level_enabled_max = level_enabled_max.max(*level as u8);
    }

    level_enabled_max
}

fn level_filter_from_str(level_str: &str) -> Option<LevelFilter> {
    let level = match level_str.to_ascii_lowercase().as_str() {
        "" | "trace" => LevelFilter::Trace,
        "debug" => LevelFilter::Debug,
        "info" => LevelFilter::Info,
        "warn" => LevelFilter::Warn,
        "error" => LevelFilter::Error,
        "off" => LevelFilter::Off,
        "disable" | "no" | "none" | "disabled" => {
            crate::warn!(
                "Invalid log level \"{level_str}\", to disable logging set to \"off\". Defaulting to \"off\"."
            );
            LevelFilter::Off
        }
        _ => {
            crate::warn!("Invalid log level \"{level_str}\", ignoring");
            return None;
        }
    };
    Some(level)
}

fn scope_alloc_from_scope_str(scope_str: &str) -> Option<ScopeAlloc> {
    let mut scope_buffer = [""; SCOPE_DEPTH_MAX];
    let mut scope_count = 0;
    let mut scope_iter = scope_str.split(SCOPE_STRING_SEP_STR);

    'scope_buffer: for entry in &mut scope_buffer {
        for scope_name in scope_iter.by_ref() {
            if scope_name.is_empty() {
                continue;
            }
            *entry = scope_name;
            scope_count += 1;
            continue 'scope_buffer;
        }
        break;
    }

    if scope_count == 0 {
        return None;
    }

    if scope_iter.next().is_some() {
        crate::warn!(
            "Invalid scope key, too many nested scopes: '{scope_str}'. Max depth is {SCOPE_DEPTH_MAX}",
        );
        return None;
    }

    let scope = scope_buffer.map(ToString::to_string);
    Some(scope)
}

#[derive(Debug, PartialEq, Eq)]
pub struct ScopeMapEntry {
    scope: String,
    enabled: Option<LevelFilter>,
    descendants: Range<usize>,
}

struct ProcessQueueEntry {
    parent_index: usize,
    depth: usize,
    items_range: Range<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnabledStatus {
    Enabled,
    Disabled,
    NotConfigured,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ScopeMap {
    entries: Vec<ScopeMapEntry>,
    modules: Vec<(String, LevelFilter)>,
    root_count: usize,
}

impl ScopeMap {
    pub fn new_from_settings_and_env<S: std::hash::BuildHasher>(
        items_input_map: &HashMap<String, String, S>,
        env_config: Option<&EnvFilter>,
        default_filters: &[(&str, LevelFilter)],
    ) -> Self {
        let mut items = Vec::<(ScopeAlloc, LevelFilter)>::with_capacity(
            items_input_map.len()
                + env_config.map_or(0, |env_filter| env_filter.directive_names.len())
                + default_filters.len(),
        );
        let mut modules = Vec::with_capacity(4);

        let env_filters = env_config.iter().flat_map(|env_filter| {
            env_filter
                .directive_names
                .iter()
                .zip(env_filter.directive_levels.iter())
                .map(|(scope_str, level_filter)| (scope_str.as_str(), *level_filter))
        });

        let new_filters = items_input_map.iter().filter_map(|(scope_str, level_str)| {
            let level_filter = level_filter_from_str(level_str)?;
            Some((scope_str.as_str(), level_filter))
        });

        let all_filters = default_filters
            .iter()
            .copied()
            .chain(env_filters)
            .chain(new_filters);

        for (scope_str, level_filter) in all_filters {
            if scope_str.contains("::") {
                if let Some((_, module_level_filter)) =
                    modules.iter_mut().find(|(module, _)| module == scope_str)
                {
                    *module_level_filter = level_filter;
                } else {
                    modules.push((scope_str.to_string(), level_filter));
                }
                continue;
            }
            let Some(scope) = scope_alloc_from_scope_str(scope_str) else {
                continue;
            };
            if let Some((_, item_level_filter)) = items
                .iter_mut()
                .find(|(scope_existing, _)| scope_existing == &scope)
            {
                *item_level_filter = level_filter;
            } else {
                items.push((scope, level_filter));
            }
        }

        items.sort_by(|left, right| left.0.cmp(&right.0));
        modules.sort_by(|(left_name, _), (right_name, _)| left_name.cmp(right_name));

        let mut this = Self {
            entries: Vec::with_capacity(items.len() * SCOPE_DEPTH_MAX),
            modules,
            root_count: 0,
        };

        let items_count = items.len();

        let mut process_queue = VecDeque::new();
        process_queue.push_back(ProcessQueueEntry {
            parent_index: usize::MAX,
            depth: 0,
            items_range: 0..items_count,
        });

        let empty_range = 0..0;

        while let Some(process_entry) = process_queue.pop_front() {
            let ProcessQueueEntry {
                items_range,
                depth,
                parent_index,
            } = process_entry;
            let mut cursor = items_range.start;
            let result_entries_start = this.entries.len();

            while cursor < items_range.end {
                let sub_items_start = cursor;
                cursor += 1;
                let (scope, level_filter) = items
                    .get(sub_items_start)
                    .expect("scope item should be in bounds");
                let scope_name = scope.get(depth).expect("scope depth should be in bounds");

                while cursor < items_range.end {
                    let cursor_scope_name = items
                        .get(cursor)
                        .expect("scope item should be in bounds")
                        .0
                        .get(depth)
                        .expect("scope depth should be in bounds");
                    if cursor_scope_name != scope_name {
                        break;
                    }
                    cursor += 1;
                }

                let sub_items_end = cursor;
                if scope_name.is_empty() {
                    assert_eq!(sub_items_start + 1, sub_items_end);
                    assert_ne!(depth, 0);
                    assert_ne!(parent_index, usize::MAX);
                    let parent_entry = this
                        .entries
                        .get_mut(parent_index)
                        .expect("parent entry should be in bounds");
                    assert!(parent_entry.enabled.is_none());
                    parent_entry.enabled = Some(*level_filter);
                    continue;
                }
                let is_last = depth + 1 == SCOPE_DEPTH_MAX;
                let mut enabled = None;

                if is_last {
                    let items_in_range = items
                        .get(items_range.clone())
                        .expect("items range should be in bounds");
                    assert_eq!(
                        sub_items_start + 1,
                        sub_items_end,
                        "expected one item: got {items_in_range:?}",
                    );
                    enabled = Some(*level_filter);
                } else {
                    let entry_index = this.entries.len();
                    process_queue.push_back(ProcessQueueEntry {
                        items_range: sub_items_start..sub_items_end,
                        parent_index: entry_index,
                        depth: depth + 1,
                    });
                }

                this.entries.push(ScopeMapEntry {
                    scope: scope_name.to_owned(),
                    enabled,
                    descendants: empty_range.clone(),
                });
            }

            let result_entries_end = this.entries.len();

            if parent_index == usize::MAX {
                this.root_count = result_entries_end;
            } else {
                let parent_entry = this
                    .entries
                    .get_mut(parent_index)
                    .expect("parent entry should be in bounds");
                parent_entry.descendants = result_entries_start..result_entries_end;
            }
        }

        this
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.modules.is_empty()
    }

    pub fn is_enabled<S>(
        &self,
        scope: &[S; SCOPE_DEPTH_MAX],
        module_path: Option<&str>,
        level: Level,
    ) -> EnabledStatus
    where
        S: AsRef<str>,
    {
        fn search<S>(map: &ScopeMap, scope: &[S; SCOPE_DEPTH_MAX]) -> Option<LevelFilter>
        where
            S: AsRef<str>,
        {
            let mut enabled = None;
            let mut current_range = map.entries.get(..map.root_count)?;
            let mut scope_names = scope.iter().map(AsRef::as_ref);
            'search: while !current_range.is_empty() {
                let Some(scope_name) = scope_names.next() else {
                    break 'search;
                };
                if scope_name.is_empty() {
                    break 'search;
                }

                for entry in current_range {
                    if entry.scope == scope_name {
                        enabled = entry.enabled.or(enabled);
                        current_range = map.entries.get(entry.descendants.clone())?;
                        continue 'search;
                    }
                }
                break 'search;
            }
            enabled
        }

        let mut enabled = search(self, scope);

        if let Some(module_path) = module_path {
            let scope_name = scope
                .first()
                .expect("scope should have first entry")
                .as_ref();

            if enabled.is_none() && scope_name.is_empty() {
                let crate_name = crate::private::extract_crate_name_from_module_path(module_path);
                let mut crate_name_scope = [""; SCOPE_DEPTH_MAX];
                let crate_name_entry = crate_name_scope
                    .first_mut()
                    .expect("scope should have first entry");
                *crate_name_entry = crate_name;
                enabled = search(self, &crate_name_scope);
            }

            if !self.modules.is_empty() {
                let crate_name = crate::private::extract_crate_name_from_module_path(module_path);
                let scope_next_name = scope
                    .get(1)
                    .expect("scope should have second entry")
                    .as_ref();
                let is_scope_just_crate_name =
                    scope_name == crate_name && scope_next_name.is_empty();
                if enabled.is_none() || is_scope_just_crate_name {
                    for (module, filter) in &self.modules {
                        if module == module_path {
                            enabled.replace(*filter);
                            break;
                        }
                    }
                }
            }
        }

        if let Some(enabled_filter) = enabled {
            if level <= enabled_filter {
                return EnabledStatus::Enabled;
            }
            return EnabledStatus::Disabled;
        }
        EnabledStatus::NotConfigured
    }

    const fn empty() -> ScopeMap {
        ScopeMap {
            entries: vec![],
            modules: vec![],
            root_count: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{Scope, env_config, private::scope_new};

    fn scope_map_from_settings(settings_entries: &[(&str, &str)]) -> ScopeMap {
        let hash_map: HashMap<String, String> = settings_entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        ScopeMap::new_from_settings_and_env(&hash_map, None, &[])
    }

    fn level_filter_from_u8(level: u8) -> LevelFilter {
        match level {
            level if level == LevelFilter::Off as u8 => LevelFilter::Off,
            level if level == LevelFilter::Error as u8 => LevelFilter::Error,
            level if level == LevelFilter::Warn as u8 => LevelFilter::Warn,
            level if level == LevelFilter::Info as u8 => LevelFilter::Info,
            level if level == LevelFilter::Debug as u8 => LevelFilter::Debug,
            level if level == LevelFilter::Trace as u8 => LevelFilter::Trace,
            _ => panic!("invalid LevelFilter value: {level}"),
        }
    }

    fn scope_from_scope_str(scope_str: &'static str) -> Scope {
        let mut scope_buffer = [""; SCOPE_DEPTH_MAX];
        let mut index = 0;
        let mut scope_iter = scope_str.split(crate::SCOPE_STRING_SEP_STR);

        while index < SCOPE_DEPTH_MAX {
            let Some(scope_name) = scope_iter.next() else {
                break;
            };
            if scope_name.is_empty() {
                continue;
            }
            let scope_entry = scope_buffer
                .get_mut(index)
                .expect("scope index should be in bounds");
            *scope_entry = scope_name;
            index += 1;
        }

        assert_ne!(index, 0);
        assert!(scope_iter.next().is_none());
        scope_buffer
    }

    fn scope_map_from_settings_and_env(
        settings_entries: &[(&str, &str)],
        env_filter: &EnvFilter,
    ) -> ScopeMap {
        let hash_map: HashMap<String, String> = settings_entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        ScopeMap::new_from_settings_and_env(&hash_map, Some(env_filter), &[])
    }

    fn scope_map_from_all(
        settings_entries: &[(&str, &str)],
        env_filter: &EnvFilter,
        default_filters: &[(&str, LevelFilter)],
    ) -> ScopeMap {
        let hash_map: HashMap<String, String> = settings_entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        ScopeMap::new_from_settings_and_env(&hash_map, Some(env_filter), default_filters)
    }

    #[test]
    fn test_initialization() {
        let map = scope_map_from_settings(&[("a.b.c.d", "trace")]);
        assert_eq!(map.root_count, 1);
        assert_eq!(map.entries.len(), 4);

        let map = scope_map_from_settings(&[]);
        assert_eq!(map.root_count, 0);
        assert_eq!(map.entries.len(), 0);

        let map = scope_map_from_settings(&[("", "trace")]);
        assert_eq!(map.root_count, 0);
        assert_eq!(map.entries.len(), 0);

        let map = scope_map_from_settings(&[("foo..bar", "trace")]);
        assert_eq!(map.root_count, 1);
        assert_eq!(map.entries.len(), 2);

        let map = scope_map_from_settings(&[
            ("a.b.c.d", "trace"),
            ("e.f.g.h", "debug"),
            ("i.j.k.l", "info"),
            ("m.n.o.p", "warn"),
            ("q.r.s.t", "error"),
        ]);
        assert_eq!(map.root_count, 5);
        assert_eq!(map.entries.len(), 20);
        let root_scopes = map
            .entries
            .iter()
            .take(map.root_count)
            .map(|entry| entry.scope.as_str())
            .collect::<Vec<_>>();
        assert_eq!(root_scopes, ["a", "e", "i", "m", "q"]);
    }

    #[test]
    fn test_is_enabled() {
        let map = scope_map_from_settings(&[
            ("a.b.c.d", "trace"),
            ("e.f.g.h", "debug"),
            ("i.j.k.l", "info"),
            ("m.n.o.p", "warn"),
            ("q.r.s.t", "error"),
        ]);

        assert_eq!(
            map.is_enabled(&scope_from_scope_str("a.b.c.d"), None, Level::Trace),
            EnabledStatus::Enabled
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("a.b.c.d"), None, Level::Debug),
            EnabledStatus::Enabled
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("e.f.g.h"), None, Level::Debug),
            EnabledStatus::Enabled
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("e.f.g.h"), None, Level::Info),
            EnabledStatus::Enabled
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("e.f.g.h"), None, Level::Trace),
            EnabledStatus::Disabled
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("i.j.k.l"), None, Level::Info),
            EnabledStatus::Enabled
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("i.j.k.l"), None, Level::Warn),
            EnabledStatus::Enabled
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("i.j.k.l"), None, Level::Debug),
            EnabledStatus::Disabled
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("m.n.o.p"), None, Level::Warn),
            EnabledStatus::Enabled
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("m.n.o.p"), None, Level::Error),
            EnabledStatus::Enabled
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("m.n.o.p"), None, Level::Info),
            EnabledStatus::Disabled
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("q.r.s.t"), None, Level::Error),
            EnabledStatus::Enabled
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("q.r.s.t"), None, Level::Warn),
            EnabledStatus::Disabled
        );
    }

    #[test]
    fn test_is_enabled_module() {
        let mut map = scope_map_from_settings(&[("a", "trace")]);
        map.modules = [("a::b::c", "trace"), ("a::b::d", "debug")]
            .map(|(key, value)| (key.to_string(), value.parse().unwrap()))
            .to_vec();

        assert_eq!(
            map.is_enabled(
                &scope_from_scope_str("__unused__"),
                Some("a::b::c"),
                Level::Trace
            ),
            EnabledStatus::Enabled
        );
        assert_eq!(
            map.is_enabled(
                &scope_from_scope_str("__unused__"),
                Some("a::b::d"),
                Level::Debug
            ),
            EnabledStatus::Enabled
        );
        assert_eq!(
            map.is_enabled(
                &scope_from_scope_str("__unused__"),
                Some("a::b::d"),
                Level::Trace,
            ),
            EnabledStatus::Disabled
        );
        assert_eq!(
            map.is_enabled(
                &scope_from_scope_str("__unused__"),
                Some("a::e"),
                Level::Info
            ),
            EnabledStatus::NotConfigured
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("a"), Some("a::b::d"), Level::Trace),
            EnabledStatus::Disabled,
        );
        assert_eq!(
            map.is_enabled(
                &scope_from_scope_str("a.scope"),
                Some("a::b::d"),
                Level::Trace
            ),
            EnabledStatus::Enabled,
        );
    }

    #[test]
    fn test_initialization_with_env() {
        let env_filter = env_config::parse("a.b=debug,u=error").unwrap();
        let map = scope_map_from_settings_and_env(&[], &env_filter);

        assert_eq!(map.root_count, 2);
        assert_eq!(map.entries.len(), 3);
        assert_eq!(
            map.is_enabled(&scope_new(&["a"]), None, Level::Debug),
            EnabledStatus::NotConfigured
        );
        assert_eq!(
            map.is_enabled(&scope_new(&["a", "b"]), None, Level::Debug),
            EnabledStatus::Enabled
        );
        assert_eq!(
            map.is_enabled(&scope_new(&["a", "b", "c"]), None, Level::Trace),
            EnabledStatus::Disabled
        );

        let env_filter = env_config::parse("a.b=debug,e.f.g.h=trace,u=error").unwrap();
        let map = scope_map_from_settings_and_env(
            &[
                ("a.b.c.d", "trace"),
                ("e.f.g.h", "debug"),
                ("i.j.k.l", "info"),
                ("m.n.o.p", "warn"),
                ("q.r.s.t", "error"),
            ],
            &env_filter,
        );

        assert_eq!(map.root_count, 6);
        assert_eq!(map.entries.len(), 21);
        let root_scopes = map
            .entries
            .iter()
            .take(map.root_count)
            .map(|entry| entry.scope.as_str())
            .collect::<Vec<_>>();
        assert_eq!(root_scopes, ["a", "e", "i", "m", "q", "u"]);
        assert_eq!(
            map.is_enabled(&scope_new(&["a", "b", "c", "d"]), None, Level::Trace),
            EnabledStatus::Enabled
        );
        assert_eq!(
            map.is_enabled(&scope_new(&["a", "b", "c"]), None, Level::Trace),
            EnabledStatus::Disabled
        );
        assert_eq!(
            map.is_enabled(&scope_new(&["u", "v"]), None, Level::Warn),
            EnabledStatus::Disabled
        );
        assert_eq!(
            map.is_enabled(&scope_new(&["e", "f", "g", "h"]), None, Level::Trace),
            EnabledStatus::Disabled,
        );
    }

    #[test]
    fn test_precedence() {
        let default_filters = &[
            ("a.b.c", LevelFilter::Debug),
            ("p.q.r", LevelFilter::Info),
            ("x.y.z", LevelFilter::Warn),
            ("crate::module::default", LevelFilter::Error),
            ("crate::module::user", LevelFilter::Off),
        ];

        let env_filter =
            env_config::parse("a.b.c=trace,p.q=debug,m.n.o=error,crate::module::env=debug")
                .unwrap();

        let settings_filters = &[
            ("p.q.r", "trace"),
            ("m.n.o", "warn"),
            ("j.k.l", "info"),
            ("crate::module::env", "trace"),
            ("crate::module::settings", "trace"),
        ];

        let map = scope_map_from_all(settings_filters, &env_filter, default_filters);

        assert_eq!(
            map.is_enabled(&scope_from_scope_str("p.q.r"), None, Level::Trace),
            EnabledStatus::Enabled,
            "Settings should override default filters for scopes"
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("m.n.o"), None, Level::Warn),
            EnabledStatus::Enabled,
            "Settings should override env filters for scopes"
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("m.n.o"), None, Level::Debug),
            EnabledStatus::Disabled,
            "Settings correctly limit log level"
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("a.b.c"), None, Level::Trace),
            EnabledStatus::Enabled,
            "ENV should override default filters for scopes"
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("x.y.z"), None, Level::Warn),
            EnabledStatus::Enabled,
            "Default filters should work when not overridden"
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("x.y.z"), None, Level::Info),
            EnabledStatus::Disabled,
            "Default filters correctly limit log level"
        );
        assert_eq!(
            map.is_enabled(&scope_new(&[""]), Some("crate::module::env"), Level::Trace),
            EnabledStatus::Enabled,
            "Settings should override env filters for modules"
        );
        assert_eq!(
            map.is_enabled(
                &scope_new(&[""]),
                Some("crate::module::settings"),
                Level::Trace
            ),
            EnabledStatus::Enabled,
            "Settings module filters should work"
        );
        assert_eq!(
            map.is_enabled(&scope_new(&[""]), Some("crate::module::env"), Level::Debug),
            EnabledStatus::Enabled,
            "ENV should override default for modules"
        );
        assert_eq!(
            map.is_enabled(
                &scope_new(&[""]),
                Some("crate::module::default"),
                Level::Error
            ),
            EnabledStatus::Enabled,
            "Default filters should work for modules"
        );
        assert_eq!(
            map.is_enabled(
                &scope_new(&[""]),
                Some("crate::module::default"),
                Level::Warn
            ),
            EnabledStatus::Disabled,
            "Default filters correctly limit log level for modules"
        );
        assert_eq!(
            map.is_enabled(&scope_new(&[""]), Some("crate::module::user"), Level::Error),
            EnabledStatus::Disabled,
            "Module turned off in default filters is not enabled"
        );
        assert_eq!(
            map.is_enabled(
                &scope_new(&["crate"]),
                Some("crate::module::user"),
                Level::Error
            ),
            EnabledStatus::Disabled,
            "Module turned off in default filters is not enabled, even with crate name as scope"
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("a.b.c.d"), None, Level::Trace),
            EnabledStatus::Enabled,
            "Scope a.b.c should inherit from a.b env filter"
        );
        assert_eq!(
            map.is_enabled(&scope_from_scope_str("a.b.c"), None, Level::Trace),
            EnabledStatus::Enabled,
            "Scope a.b.c should use env filter level (trace)"
        );
        assert_eq!(
            map.is_enabled(&scope_new(&[""]), Some("crate::module"), Level::Error),
            EnabledStatus::NotConfigured,
            "Module crate::module should not be affected by crate::module::default filter"
        );
        assert_eq!(
            map.is_enabled(
                &scope_new(&[""]),
                Some("crate::module::default::sub"),
                Level::Error
            ),
            EnabledStatus::NotConfigured,
            "Module crate::module::default::sub should not be affected by crate::module::default filter"
        );
    }

    #[test]
    fn test_default_filter_crate() {
        let default_filters = &[("crate", LevelFilter::Off)];
        let map = scope_map_from_all(&[], &env_config::parse("").unwrap(), default_filters);

        assert_eq!(
            map.is_enabled(&scope_new(&[""]), Some("crate::submodule"), Level::Error),
            EnabledStatus::Disabled,
            "crate::submodule should be disabled by disabling `crate` filter"
        );
    }

    #[test]
    fn test_module_filters_raise_max_enabled_level() {
        for (level_str, expected_level_filter, expected_level) in [
            ("debug", LevelFilter::Debug, Level::Debug),
            ("trace", LevelFilter::Trace, Level::Trace),
        ] {
            let settings = HashMap::from([("crate::module".to_string(), level_str.to_string())]);
            assert!(
                !is_possibly_enabled_level(expected_level),
                "configured module logs should be filtered before the max enabled level is refreshed",
            );

            refresh_from_settings(&settings);

            let actual_level_filter =
                level_filter_from_u8(LEVEL_ENABLED_MAX_CONFIG.load(Ordering::Acquire));

            assert_eq!(
                actual_level_filter, expected_level_filter,
                "module-specific filters should raise the max enabled level",
            );
            assert!(
                is_possibly_enabled_level(expected_level),
                "configured module logs should survive the max enabled level check before module matching runs",
            );
            assert!(
                is_scope_enabled(&scope_new(&[""]), Some("crate::module"), expected_level),
                "exact module matching should allow the configured module level once the filter is checked",
            );
        }
    }
}
