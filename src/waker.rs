use std::cell::RefCell;
use std::task::Waker;

pub struct TaskWaker {
    waker: RefCell<Option<Waker>>,
}

impl TaskWaker {
    pub fn new() -> Self {
        Self {
            waker: RefCell::new(None),
        }
    }

    pub fn register(&self, waker: &Waker) {
        self.waker.replace(Some(waker.clone()));
    }

    pub fn wake(&self) {
        match &*self.waker.borrow() {
            Some(waker) => waker.wake_by_ref(),
            _ => {}
        }
    }
}
