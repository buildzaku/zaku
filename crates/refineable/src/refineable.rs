pub use refineable_derive::Refineable;

pub trait Refineable: Clone {
    type Refinement: Refineable<Refinement = Self::Refinement> + IsEmpty + Default;

    fn refine(&mut self, refinement: &Self::Refinement);

    fn refined(self, refinement: Self::Refinement) -> Self;

    fn from_cascade(cascade: &Cascade<Self>) -> Self
    where
        Self: Default + Sized,
    {
        Self::default().refined(cascade.merged())
    }

    fn is_superset_of(&self, refinement: &Self::Refinement) -> bool;

    fn subtract(&self, refinement: &Self::Refinement) -> Self::Refinement;
}

pub trait IsEmpty {
    fn is_empty(&self) -> bool;
}

pub struct Cascade<S: Refineable> {
    base: S::Refinement,
    refinements: Vec<Option<S::Refinement>>,
}

impl<S: Refineable> Default for Cascade<S> {
    fn default() -> Self {
        Self {
            base: Default::default(),
            refinements: Vec::new(),
        }
    }
}

#[derive(Copy, Clone)]
pub struct CascadeSlot(usize);

impl<S: Refineable> Cascade<S> {
    pub fn reserve(&mut self) -> CascadeSlot {
        self.refinements.push(None);
        CascadeSlot(self.refinements.len() - 1)
    }

    pub fn base(&mut self) -> &mut S::Refinement {
        &mut self.base
    }

    pub fn set(&mut self, slot: CascadeSlot, refinement: Option<S::Refinement>) {
        let slot_is_reserved = match self.refinements.get_mut(slot.0) {
            Some(reserved_refinement) => {
                *reserved_refinement = refinement;
                true
            }
            None => false,
        };

        assert!(slot_is_reserved, "cascade slot should be reserved");
    }

    pub fn merged(&self) -> S::Refinement {
        let mut merged = self.base.clone();
        for refinement in self.refinements.iter().flatten() {
            merged.refine(refinement);
        }
        merged
    }
}
