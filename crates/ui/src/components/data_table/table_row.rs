use std::ops::{
    Index, IndexMut, Range, RangeFrom, RangeFull, RangeInclusive, RangeTo, RangeToInclusive,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableRow<T>(Vec<T>);

impl<T> TableRow<T> {
    pub fn from_element(element: T, length: usize) -> Self
    where
        T: Clone,
    {
        Self::from_vec(vec![element; length], length)
    }

    pub fn from_vec(data: Vec<T>, expected_length: usize) -> Self {
        Self::try_from_vec(data, expected_length)
            .expect("table row length should match expected length")
    }

    pub fn try_from_vec(data: Vec<T>, expected_length: usize) -> Result<Self, String> {
        if data.len() == expected_length {
            Ok(Self(data))
        } else {
            Err(format!(
                "Row length {} does not match expected {}",
                data.len(),
                expected_length
            ))
        }
    }

    pub fn expect_get(&self, column: impl Into<usize>) -> &T {
        let column = column.into();
        self.0.get(column).expect("table row column should exist")
    }

    pub fn expect_get_mut(&mut self, column: impl Into<usize>) -> &mut T {
        let column = column.into();
        self.0
            .get_mut(column)
            .expect("table row column should exist")
    }

    pub fn get(&self, column: impl Into<usize>) -> Option<&T> {
        self.0.get(column.into())
    }

    pub fn as_slice(&self) -> &[T] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<T> {
        self.0
    }

    pub fn map_cloned<F, U>(&self, mapper: F) -> TableRow<U>
    where
        F: FnMut(T) -> U,
        T: Clone,
    {
        self.clone().map(mapper)
    }

    pub fn map<F, U>(self, mapper: F) -> TableRow<U>
    where
        F: FnMut(T) -> U,
    {
        TableRow(self.0.into_iter().map(mapper).collect())
    }

    pub fn map_ref<F, U>(&self, mapper: F) -> TableRow<U>
    where
        F: FnMut(&T) -> U,
    {
        TableRow(self.0.iter().map(mapper).collect())
    }

    pub fn cols(&self) -> usize {
        self.0.len()
    }
}

impl<T> Index<usize> for TableRow<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        self.0.get(index).expect("table row index should exist")
    }
}

impl<T> IndexMut<usize> for TableRow<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.0.get_mut(index).expect("table row index should exist")
    }
}

impl<T> Index<Range<usize>> for TableRow<T> {
    type Output = [T];

    fn index(&self, index: Range<usize>) -> &Self::Output {
        <Vec<T> as Index<Range<usize>>>::index(&self.0, index)
    }
}

impl<T> Index<RangeFrom<usize>> for TableRow<T> {
    type Output = [T];

    fn index(&self, index: RangeFrom<usize>) -> &Self::Output {
        <Vec<T> as Index<RangeFrom<usize>>>::index(&self.0, index)
    }
}

impl<T> Index<RangeTo<usize>> for TableRow<T> {
    type Output = [T];

    fn index(&self, index: RangeTo<usize>) -> &Self::Output {
        <Vec<T> as Index<RangeTo<usize>>>::index(&self.0, index)
    }
}

impl<T> Index<RangeToInclusive<usize>> for TableRow<T> {
    type Output = [T];

    fn index(&self, index: RangeToInclusive<usize>) -> &Self::Output {
        <Vec<T> as Index<RangeToInclusive<usize>>>::index(&self.0, index)
    }
}

impl<T> Index<RangeFull> for TableRow<T> {
    type Output = [T];

    fn index(&self, index: RangeFull) -> &Self::Output {
        <Vec<T> as Index<RangeFull>>::index(&self.0, index)
    }
}

impl<T> Index<RangeInclusive<usize>> for TableRow<T> {
    type Output = [T];

    fn index(&self, index: RangeInclusive<usize>) -> &Self::Output {
        <Vec<T> as Index<RangeInclusive<usize>>>::index(&self.0, index)
    }
}

impl<T> IndexMut<RangeInclusive<usize>> for TableRow<T> {
    fn index_mut(&mut self, index: RangeInclusive<usize>) -> &mut Self::Output {
        <Vec<T> as IndexMut<RangeInclusive<usize>>>::index_mut(&mut self.0, index)
    }
}

pub trait IntoTableRow<T> {
    fn into_table_row(self, expected_length: usize) -> TableRow<T>;
}

impl<T> IntoTableRow<T> for Vec<T> {
    fn into_table_row(self, expected_length: usize) -> TableRow<T> {
        TableRow::from_vec(self, expected_length)
    }
}
