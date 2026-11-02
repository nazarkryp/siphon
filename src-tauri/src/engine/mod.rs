pub mod hls;
pub mod http;

use crate::job::PartState;

/// Where an engine reads and writes the progress of one source.
pub trait PartSink: Send + Sync + 'static {
    fn snapshot(&self) -> Option<PartState>;
    fn update(&self, f: &mut dyn FnMut(&mut Option<PartState>));
}

#[cfg(test)]
pub mod testing {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    pub struct MemorySink(pub Mutex<Option<PartState>>);

    impl PartSink for MemorySink {
        fn snapshot(&self) -> Option<PartState> {
            self.0.lock().unwrap().clone()
        }

        fn update(&self, f: &mut dyn FnMut(&mut Option<PartState>)) {
            f(&mut self.0.lock().unwrap());
        }
    }
}
