use std::{collections::VecDeque, time::Duration};

use bevy_ecs::prelude::*;

use audio_graph::{AgEvent, Node, ProcessContext, Processor};
use derivative::Derivative;

use crate::midi::MidiReceiver;

#[derive(Debug)]
pub struct MidiInputNode {
    pub entity: Entity,
}

impl MidiInputNode {
    pub fn new(world: &mut World) -> Self {
        let entity = world.spawn(Node::default().event(0, 1).always_run()).id();
        audio_graph::set_processor(world, entity, Box::new(MidiInputProcessor::default()));
        MidiInputNode { entity }
    }
}

#[derive(Derivative)]
#[derivative(Debug)]
struct MidiInputProcessor {
    #[derivative(Debug = "ignore")]
    midi_receiver: Option<MidiReceiver>,

    #[derivative(Debug = "ignore")]
    events: VecDeque<AgEvent>,

    first_event_timestamp: Option<(u64, Duration)>,
}

impl Default for MidiInputProcessor {
    fn default() -> Self {
        let midi_receiver = MidiReceiver::new();
        if let Err(err) = &midi_receiver {
            println!("** Failed to create MIDI receiver: {}", err);
        }
        let midi_receiver = midi_receiver.ok().flatten();

        Self {
            midi_receiver,
            events: Default::default(),
            first_event_timestamp: Default::default(),
        }
    }
}

impl Processor for MidiInputProcessor {
    fn process(&mut self, ctx: ProcessContext) {
        self.receive_midi_events(ctx.timestamp);
        ctx.out_event_buffers[0].extend(self.events.iter().cloned());
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

            let session_time = session_time.max(*timestamp);

            self.events.push_back(AgEvent {
                timestamp: session_time,
                midi: event.midi_event,
            })
        }
    }
}
