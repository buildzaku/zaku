use gpui::{AnyElement, App, SharedString, Window};
use parking_lot::RwLock;
use std::{collections::HashMap, sync::LazyLock};
use strum::{Display, EnumString};

pub fn components() -> ComponentRegistry {
    COMPONENT_DATA.read().clone()
}

pub fn init() {
    for f in inventory::iter::<ComponentFn>() {
        (f.0)();
    }
}

pub struct ComponentFn(fn());

impl ComponentFn {
    pub const fn new(f: fn()) -> Self {
        Self(f)
    }
}

inventory::collect!(ComponentFn);

#[doc(hidden)]
pub mod __private {
    pub use inventory;
}

pub fn register_component<T: Component>() {
    let id = T::id();
    let metadata = ComponentMetadata {
        id: id.clone(),
        description: T::description().map(Into::into),
        name: SharedString::new_static(T::name()),
        preview: Some(T::preview),
        scope: T::scope(),
        sort_name: SharedString::new_static(T::sort_name()),
        status: T::status(),
    };

    let mut data = COMPONENT_DATA.write();
    data.components.insert(id, metadata);
}

pub static COMPONENT_DATA: LazyLock<RwLock<ComponentRegistry>> =
    LazyLock::new(|| RwLock::new(ComponentRegistry::default()));

#[derive(Default, Clone)]
pub struct ComponentRegistry {
    components: HashMap<ComponentId, ComponentMetadata>,
}

impl ComponentRegistry {
    pub fn previews(&self) -> Vec<&ComponentMetadata> {
        self.components
            .values()
            .filter(|c| c.preview.is_some())
            .collect()
    }

    pub fn sorted_previews(&self) -> Vec<ComponentMetadata> {
        let mut previews: Vec<ComponentMetadata> = self.previews().into_iter().cloned().collect();
        previews.sort_by_key(|a| a.name());
        previews
    }

    pub fn components(&self) -> Vec<&ComponentMetadata> {
        self.components.values().collect()
    }

    pub fn sorted_components(&self) -> Vec<ComponentMetadata> {
        let mut components: Vec<ComponentMetadata> =
            self.components().into_iter().cloned().collect();
        components.sort_by_key(|a| a.name());
        components
    }

    pub fn component_map(&self) -> HashMap<ComponentId, ComponentMetadata> {
        self.components.clone()
    }

    pub fn get(&self, id: &ComponentId) -> Option<&ComponentMetadata> {
        self.components.get(id)
    }

    pub fn len(&self) -> usize {
        self.components.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentId(pub &'static str);

#[derive(Clone)]
pub struct ComponentMetadata {
    id: ComponentId,
    description: Option<SharedString>,
    name: SharedString,
    preview: Option<fn(&mut Window, &mut App) -> Option<AnyElement>>,
    scope: ComponentScope,
    sort_name: SharedString,
    status: ComponentStatus,
}

impl ComponentMetadata {
    pub fn id(&self) -> ComponentId {
        self.id.clone()
    }

    pub fn description(&self) -> Option<SharedString> {
        self.description.clone()
    }

    pub fn name(&self) -> SharedString {
        self.name.clone()
    }

    pub fn preview(&self) -> Option<fn(&mut Window, &mut App) -> Option<AnyElement>> {
        self.preview
    }

    pub fn scope(&self) -> ComponentScope {
        self.scope.clone()
    }

    pub fn sort_name(&self) -> SharedString {
        self.sort_name.clone()
    }

    pub fn scopeless_name(&self) -> SharedString {
        self.name
            .clone()
            .split("::")
            .last()
            .unwrap_or(&self.name)
            .to_string()
            .into()
    }

    pub fn status(&self) -> ComponentStatus {
        self.status.clone()
    }
}

/// Metadata for UI components, useful for visual debugging.
pub trait Component {
    fn id() -> ComponentId {
        ComponentId(Self::name())
    }

    fn scope() -> ComponentScope {
        ComponentScope::None
    }

    fn status() -> ComponentStatus {
        ComponentStatus::Live
    }

    fn name() -> &'static str {
        std::any::type_name::<Self>()
    }

    fn sort_name() -> &'static str {
        Self::name()
    }

    fn description() -> Option<&'static str> {
        None
    }

    fn preview(_window: &mut Window, _cx: &mut App) -> Option<AnyElement> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, EnumString)]
pub enum ComponentStatus {
    #[strum(serialize = "Work In Progress")]
    WorkInProgress,
    #[strum(serialize = "Ready To Build")]
    EngineeringReady,
    Live,
    Deprecated,
}

impl ComponentStatus {
    pub fn description(&self) -> &str {
        match self {
            ComponentStatus::WorkInProgress => {
                "These components are still being designed or refined. They shouldn't be used in the app yet."
            }
            ComponentStatus::EngineeringReady => {
                "These components are design complete or partially implemented, and are ready for an engineer to complete their implementation."
            }
            ComponentStatus::Live => "These components are ready for use in the app.",
            ComponentStatus::Deprecated => {
                "These components are no longer recommended for use in the app, and may be removed in a future release."
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, EnumString)]
pub enum ComponentScope {
    #[strum(serialize = "Images & Icons")]
    Images,
    #[strum(serialize = "Data Display")]
    DataDisplay,
    #[strum(serialize = "Forms & Input")]
    Input,
    #[strum(serialize = "Layout & Structure")]
    Layout,
    #[strum(serialize = "Loading & Progress")]
    Loading,
    Navigation,
    #[strum(serialize = "Unsorted")]
    None,
    Notification,
    #[strum(serialize = "Overlays & Layering")]
    Overlays,
    Status,
    Typography,
    #[strum(serialize = "Version Control")]
    VersionControl,
}
