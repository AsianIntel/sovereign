use std::marker::PhantomData;

#[derive(Debug, Eq, PartialEq)]
pub struct Handle<T> {
    pub idx: usize,
    _p: PhantomData<T>,
}

pub struct Assets<T> {
    items: Vec<T>,
    count: usize,
}

impl<T> Assets<T> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            count: 0,
        }
    }

    pub fn push(&mut self, item: T) -> Handle<T> {
        self.items.push(item);
        let idx = self.count;
        self.count += 1;
        Handle {
            idx,
            _p: PhantomData,
        }
    }

    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        self.items.get(handle.idx)
    }
}

impl<T> Copy for Handle<T> {}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Self {
            idx: self.idx,
            _p: PhantomData,
        }
    }
}
