use std::time::Duration;

use wmidi::MidiMessage;

pub struct Event {
    pub timestamp: Duration,
    pub midi: MidiMessage<'static>,
}
