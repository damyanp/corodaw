use std::time::Duration;

use wmidi::MidiMessage;

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub timestamp: Duration,
    pub midi: MidiMessage<'static>,
}
