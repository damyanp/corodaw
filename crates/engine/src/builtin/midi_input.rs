use std::{cell::RefCell, collections::VecDeque, time::Duration};

use audio_blocks::AudioBlockSequential;
use audio_graph::{AudioGraph, Event, Graph, Node, NodeId, Processor};
use derivative::Derivative;

use crate::midi::MidiReceiver;

#[derive(Debug)]
pub struct MidiInputNode {
    pub node_id: NodeId,
}

impl MidiInputNode {
    pub fn new(graph: &AudioGraph) -> Self {
        let processor = Box::new(MidiInputProcessor::default());

        let node_id = graph.add_node(0, 0, processor);
        MidiInputNode { node_id }
    }
}

// TOOD: all these RefCells are silly. Can't we just make process give us a &mut self?
#[derive(Derivative)]
#[derivative(Debug)]
struct MidiInputProcessor {
    #[derivative(Debug = "ignore")]
    midi_receiver: Option<RefCell<MidiReceiver>>,

    #[derivative(Debug = "ignore")]
    events: RefCell<VecDeque<Event>>,

    first_event_timestamp: RefCell<Option<(u64, Duration)>>,
}

impl Default for MidiInputProcessor {
    fn default() -> Self {
        Self {
            midi_receiver: MidiReceiver::new().ok().flatten().map(RefCell::new),
            events: Default::default(),
            first_event_timestamp: Default::default(),
        }
    }
}

impl Processor for MidiInputProcessor {
    fn process(
        &self,
        _: &Graph,
        _: &Node,
        timestamp: &Duration,
        _: &mut [AudioBlockSequential<f32>],
    ) {
        self.receive_midi_events(timestamp);

        let mut events = self.events.borrow_mut();
        for event in events.iter() {
            println!("{:?}: {:?}", event.timestamp, event.midi);
        }
        events.clear();
    }
}

impl MidiInputProcessor {
    fn receive_midi_events(&self, timestamp: &Duration) {
        let Some(midi_receiver) = self.midi_receiver.as_ref() else {
            return;
        };

        let mut midi_receiver = midi_receiver.borrow_mut();
        let Some(events) = midi_receiver.receive_all_events() else {
            return;
        };

        let mut first_event_timestamp = self.first_event_timestamp.borrow_mut();

        let mut self_events = self.events.borrow_mut();

        for event in events {
            let (start_midi_time, start_session_time) =
                *first_event_timestamp.get_or_insert_with(|| (event.timestamp, *timestamp));

            let micros_since_midi_start = event.timestamp - start_midi_time;
            let since_midi_start = Duration::from_micros(micros_since_midi_start);
            let session_time = start_session_time + since_midi_start;

            self_events.push_back(Event {
                timestamp: session_time,
                midi: event.midi_event,
            })
        }
    }
}
