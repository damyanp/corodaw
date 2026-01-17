#![allow(unreachable_code)]
// Adapted from https://github.com/prokopyl/clack/blob/main/host/examples/cpal/src/host/audio/midi.rs

use midir::{Ignore, MidiInput, MidiInputConnection};
use rtrb::{Consumer, Producer, RingBuffer, chunks::ReadChunkIntoIter};
use std::error::Error;
use wmidi::MidiMessage;

/// A MIDI message that was received at a given time.
pub struct MidiEventMessage {
    /// A micro-timestamp of when the event occurred.
    ///
    /// This is given by `midir` and is unrelated to the audio frame counter. It is based off an
    /// arbitrary start time. The only guarantee is that this timestamp is steadily increasing.
    pub timestamp: u64,
    /// The MIDI event. This is 'static to make it simpler to share across threads, meaning we
    /// don't support MIDI SysEx messages.
    pub midi_event: MidiMessage<'static>,
}

/// A receiver for the MIDI event stream.
///
/// This is to be held by the audio thread, and will collect events from the MIDI thread.
pub struct MidiReceiver {
    /// The input connection to the MIDI device.
    /// This isn't used directly, but must be kept alive to ensure keep the connection open.
    _connection: MidiInputConnection<MidiReceiverWorker>,

    /// The consumer side of the ring buffer the MIDI thread sends event through.
    consumer: Consumer<MidiEventMessage>,
}

impl MidiReceiver {
    /// Connects to a MIDI device and starts receiving events.
    ///
    /// This selects the last MIDI device that was plugged in, if any.
    pub fn new() -> Result<Option<Self>, Box<dyn Error>> {
        let mut input = MidiInput::new("corodaw")?;
        input.ignore(Ignore::Sysex | Ignore::Time | Ignore::ActiveSense);

        let ports = input.ports();

        if ports.is_empty() {
            println!("No MIDI input device found. Plugin will not be fed any MIDI input.");
            return Ok(None);
        }

        // PANIC: we checked ports wasn't empty above
        let selected_port = ports.last().unwrap();
        let port_name = input.port_name(selected_port)?;

        if ports.len() > 1 {
            println!("Found multiple MIDI input ports:");
            for x in &ports {
                let Ok(port_name) = input.port_name(x) else {
                    continue;
                };
                println!("\t > {port_name}")
            }

            println!("\t * Using the latest MIDI device as input: {port_name}");
        } else {
            println!("MIDI device found! Using '{port_name}' as input.");
        }

        let (producer, consumer) = RingBuffer::new(128);

        let worker = MidiReceiverWorker {
            first_timestamp: None,
            producer,
        };

        let connection = input.connect(
            selected_port,
            "corodaw MIDI input",
            |timestamp, data, worker| {
                worker.receive_event(timestamp, data);
            },
            worker,
        )?;

        Ok(Some(Self {
            _connection: connection,
            consumer,
        }))
    }

    /// Receives all the MIDI events since the last call to the method.
    pub fn receive_all_events<'a>(&'a mut self) -> Option<ReadChunkIntoIter<'a, MidiEventMessage>> {
        if self.consumer.is_abandoned() {
            None
        } else {
            let slots = self.consumer.slots();
            self.consumer
                .read_chunk(slots)
                .ok()
                .map(|chunk| chunk.into_iter())
        }
    }
}

struct MidiReceiverWorker {
    first_timestamp: Option<u64>,
    producer: Producer<MidiEventMessage>,
}

impl MidiReceiverWorker {
    fn receive_event(&mut self, timestamp: u64, data: &[u8]) {
        let first_timestamp = *self.first_timestamp.get_or_insert(timestamp);
        assert!(timestamp >= first_timestamp);

        let Ok(midi_event) = MidiMessage::try_from(data) else {
            return;
        };

        let Some(midi_event) = midi_event.drop_unowned_sysex() else {
            return;
        };

        let _ = self.producer.push(MidiEventMessage {
            timestamp: timestamp - first_timestamp,
            midi_event,
        });
    }
}
