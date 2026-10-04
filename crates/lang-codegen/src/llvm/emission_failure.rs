//! Thread-local object-path fault injection, compiled exclusively into tests.

use std::{cell::RefCell, path::{Path, PathBuf}};

thread_local! {
    static FAILURE: RefCell<Option<(PathBuf, usize)>> = const { RefCell::new(None) };
}

pub(crate) struct Guard(Option<(PathBuf, usize)>);

impl Guard {
    pub(crate) fn new(path: PathBuf) -> Self {
        assert!(path.parent().expect("fault parent").is_file());
        Self(FAILURE.with(|state| state.replace(Some((path, 0)))))
    }

    pub(crate) fn calls(&self) -> usize {
        FAILURE.with(|state| state.borrow().as_ref().expect("live guard").1)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        FAILURE.with(|state| state.replace(self.0.take()));
    }
}

pub(super) fn object_path(path: &Path) -> PathBuf {
    FAILURE.with(|state| {
        let mut state = state.borrow_mut();
        match state.as_mut() {
            Some((fault, count)) => {
                *count += 1;
                fault.clone()
            }
            None => path.to_path_buf(),
        }
    })
}
