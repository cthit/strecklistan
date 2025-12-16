use std::cmp::{Eq, Ord, Ordering, PartialEq, PartialOrd};

/// Make some type Ord if it is only PartialOrd, e.g. floats.
///
/// Unorderable values (e.g. NaNs) are always compared to as being less than other values.
/// This breaks commutativity, but is fine for the simple case.
#[derive(PartialEq, Clone, Copy, Debug)]
pub struct OrdL<T>(pub T);

impl<T> Eq for OrdL<T> where T: PartialEq {}

impl<T> PartialOrd for OrdL<T>
where
    T: PartialOrd,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for OrdL<T>
where
    T: PartialOrd,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.partial_cmp(&other.0).unwrap_or(Ordering::Less)
    }
}
