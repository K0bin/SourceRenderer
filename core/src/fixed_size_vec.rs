use smallvec::SmallVec;

pub struct FixedSizeSmallVec<T: smallvec::Array>(SmallVec<T>);
impl<T: smallvec::Array> FixedSizeSmallVec<T> {
    pub fn new() -> Self {
        Self(SmallVec::new())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self(SmallVec::with_capacity(capacity))
    }

    pub fn push(&mut self, value: T::Item) {
        let ptr = self.0.as_ptr();
        self.0.push(value);
        assert_eq!(self.0.as_ptr(), ptr);
    }
}

impl<T: smallvec::Array> AsRef<SmallVec<T>> for FixedSizeSmallVec<T> {
    fn as_ref(&self) -> &SmallVec<T> {
        &self.0
    }
}

impl<T: smallvec::Array> From<SmallVec<T>> for FixedSizeSmallVec<T> {
    fn from(value: SmallVec<T>) -> Self {
        Self(value)
    }
}

pub struct FixedSizeVec<T>(Vec<T>);
impl<T: smallvec::Array> FixedSizeVec<T> {
    pub fn new(capacity: usize) -> Self {
        Self(Vec::with_capacity(capacity))
    }

    pub fn push(&mut self, value: T) {
        let ptr = self.0.as_ptr();
        self.0.push(value);
        assert_eq!(self.0.as_ptr(), ptr);
    }
}

impl<T> AsRef<Vec<T>> for FixedSizeVec<T> {
    fn as_ref(&self) -> &Vec<T> {
        &self.0
    }
}

impl<T> From<Vec<T>> for FixedSizeVec<T> {
    fn from(value: Vec<T>) -> Self {
        Self(value)
    }
}
