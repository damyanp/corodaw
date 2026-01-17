use std::{collections::VecDeque, time::Duration};

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

#[derive(Derivative)]
#[derivative(Debug)]
struct MidiInputProcessor {
    #[derivative(Debug = "ignore")]
    midi_receiver: Option<MidiReceiver>,

    #[derivative(Debug = "ignore")]
    events: VecDeque<Event>,

    first_event_timestamp: Option<(u64, Duration)>,
}

impl Default for MidiInputProcessor {
    fn default() -> Self {
        Self {
            midi_receiver: MidiReceiver::new().ok().flatten(),
            events: Default::default(),
            first_event_timestamp: Default::default(),
        }
    }
}

impl Processor for MidiInputProcessor {
    fn process(
        &mut self,
        _: &Graph,
        _: &Node,
        timestamp: &Duration,
        _: &mut [AudioBlockSequential<f32>],
    ) {
        self.receive_midi_events(timestamp);

        for event in self.events.iter() {
            println!("{:?}: {:?}", event.timestamp, event.midi);
        }
        self.events.clear();
    }
}

impl MidiInputProcessor {
    fn receive_midi_events(&mut self, timestamp: &Duration) {
        let Some(midi_receiver) = self.midi_receiver.as_mut() else {
            return;
        };

        let Some(events) = midi_receiver.receive_all_events() else {
            return;
        };

        for event in events {
            let (start_midi_time, start_session_time) = *self
                .first_event_timestamp
                .get_or_insert((event.timestamp, *timestamp));

            let micros_since_midi_start = event.timestamp - start_midi_time;
            let since_midi_start = Duration::from_micros(micros_since_midi_start);
            let session_time = start_session_time + since_midi_start;

            self.events.push_back(Event {
                timestamp: session_time,
                midi: event.midi_event,
            })
        }
    }
}
