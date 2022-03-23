use alvr_events::Event;
use std::collections::VecDeque;

pub struct LogsPanel {
    buffer: VecDeque<Event>,
}

impl LogsPanel {
    pub fn new() -> Self {
        Self {
            buffer: VecDeque::new(),
        }
    }
}
