use gpui::Pixels;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub trait MergeFrom {
    fn merge_from(&mut self, other: &Self);

    fn merge_from_option(&mut self, other: Option<&Self>) {
        if let Some(other) = other {
            self.merge_from(other);
        }
    }
}

macro_rules! merge_from_overwrites {
    ($($type:ty),+ $(,)?) => {
        $(
            impl MergeFrom for $type {
                fn merge_from(&mut self, other: &Self) {
                    self.clone_from(other);
                }
            }
        )+
    }
}

merge_from_overwrites!(
    u16,
    u32,
    u64,
    usize,
    i16,
    i32,
    i64,
    bool,
    f64,
    f32,
    char,
    std::num::NonZeroUsize,
    std::num::NonZeroU32,
    String,
    std::path::PathBuf,
    std::sync::Arc<str>,
    std::sync::Arc<std::path::Path>,
    Pixels,
    crate::UiDensity,
);

impl<T: Clone + MergeFrom> MergeFrom for Option<T> {
    fn merge_from(&mut self, other: &Self) {
        let Some(other) = other else {
            return;
        };

        if let Some(this) = self.as_mut() {
            this.merge_from(other);
        } else {
            self.replace(other.clone());
        }
    }
}

impl<T: Clone> MergeFrom for Vec<T> {
    fn merge_from(&mut self, other: &Self) {
        self.clone_from(other);
    }
}

impl<T: MergeFrom> MergeFrom for Box<T> {
    fn merge_from(&mut self, other: &Self) {
        self.as_mut().merge_from(other.as_ref());
    }
}

impl<K, V, S> MergeFrom for HashMap<K, V, S>
where
    K: Clone + std::hash::Hash + Eq,
    V: Clone + MergeFrom,
    S: std::hash::BuildHasher,
{
    fn merge_from(&mut self, other: &Self) {
        for (key, value) in other {
            if let Some(existing) = self.get_mut(key) {
                existing.merge_from(value);
            } else {
                self.insert(key.clone(), value.clone());
            }
        }
    }
}

impl<K, V> MergeFrom for BTreeMap<K, V>
where
    K: Clone + std::hash::Hash + Eq + Ord,
    V: Clone + MergeFrom,
{
    fn merge_from(&mut self, other: &Self) {
        for (key, value) in other {
            if let Some(existing) = self.get_mut(key) {
                existing.merge_from(value);
            } else {
                self.insert(key.clone(), value.clone());
            }
        }
    }
}

impl<K, V> MergeFrom for indexmap::IndexMap<K, V>
where
    K: Clone + std::hash::Hash + Eq,
    V: Clone + MergeFrom,
{
    fn merge_from(&mut self, other: &Self) {
        for (key, value) in other {
            if let Some(existing) = self.get_mut(key) {
                existing.merge_from(value);
            } else {
                self.insert(key.clone(), value.clone());
            }
        }
    }
}

impl<T> MergeFrom for BTreeSet<T>
where
    T: Clone + Ord,
{
    fn merge_from(&mut self, other: &Self) {
        for item in other {
            self.insert(item.clone());
        }
    }
}

impl<T, S> MergeFrom for HashSet<T, S>
where
    T: Clone + std::hash::Hash + Eq,
    S: std::hash::BuildHasher,
{
    fn merge_from(&mut self, other: &Self) {
        for item in other {
            self.insert(item.clone());
        }
    }
}

impl MergeFrom for serde_json::Value {
    fn merge_from(&mut self, other: &Self) {
        match (self, other) {
            (serde_json::Value::Object(this), serde_json::Value::Object(other)) => {
                for (key, value) in other {
                    if let Some(existing) = this.get_mut(key) {
                        existing.merge_from(value);
                    } else {
                        this.insert(key.clone(), value.clone());
                    }
                }
            }
            (this, other) => *this = other.clone(),
        }
    }
}
