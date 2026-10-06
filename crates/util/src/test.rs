mod marked_text;

#[cfg(any(test, feature = "test"))]
use rand::{Rng, RngExt, seq::IndexedRandom};

pub use marked_text::{
    TextRangeMarker, generate_marked_text, marked_text_offsets, marked_text_offsets_by,
    marked_text_ranges, marked_text_ranges_by,
};

#[cfg(any(test, feature = "test"))]
pub struct RandomCharIter<T: Rng> {
    rng: T,
}

#[cfg(any(test, feature = "test"))]
impl<T: Rng> RandomCharIter<T> {
    pub fn new(rng: T) -> Self {
        Self { rng }
    }
}

#[cfg(any(test, feature = "test"))]
impl<T: Rng> Iterator for RandomCharIter<T> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        match self.rng.random_range(0..100) {
            0..=19 => [' ', '\n', '\r', '\t'].choose(&mut self.rng).copied(),
            20..=32 => char::from_u32(self.rng.random_range(u32::from('α')..=u32::from('ω'))),
            33..=45 => ['✋', '✅', '❌', '❎', '⭐']
                .choose(&mut self.rng)
                .copied(),
            46..=58 => ['🍐', '🏀', '🍗', '🎉'].choose(&mut self.rng).copied(),
            _ => Some(char::from(self.rng.random_range(b'a'..=b'z'))),
        }
    }
}
