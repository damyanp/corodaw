use std::{fmt::Debug, time::Duration};

use audio_blocks::AudioBlockSequential;
use clack_host::{
    events::event_types::MidiEvent,
    prelude::{
        AudioPortBuffer, AudioPortBufferType, AudioPorts, EventBuffer, InputAudioBuffers,
        OutputEvents,
    },
    process::PluginAudioProcessor,
};

use crate::plugins::ClapPlugin;
use audio_graph::{Event, Graph, InputConnection, Node, Processor};

pub struct ClapPluginProcessor {
    plugin_audio_processor: PluginAudioProcessor<ClapPlugin>,
    audio_ports: AudioPorts,
    input_events: EventBuffer,
    num_outputs: usize,
}

impl Debug for ClapPluginProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClapPluginProcessor").finish()
    }
}

impl ClapPluginProcessor {
    pub fn new(clap_plugin: &ClapPlugin) -> Self {
        let output_ports = clap_plugin.get_audio_ports(false);
        let total_channel_count = output_ports
            .iter()
            .copied()
            .reduce(|a, b| a + b)
            .unwrap_or(0) as usize;

        let audio_ports = AudioPorts::with_capacity(total_channel_count, output_ports.len());

        Self {
            plugin_audio_processor: clap_plugin.get_audio_processor(),
            audio_ports,
            input_events: EventBuffer::new(),
            num_outputs: total_channel_count,
        }
    }

    pub fn get_total_output_channels(&self) -> usize {
        self.num_outputs
    }
}

impl Processor for ClapPluginProcessor {
    fn process(
        &mut self,
        graph: &Graph,
        node: &Node,
        _: usize,
        timestamp: &Duration,
        out_audio_buffers: &mut [AudioBlockSequential<f32>],
        _: &mut [Vec<Event>],
    ) {
        self.update_input_events(graph, node, timestamp);

        let processor = if self.plugin_audio_processor.is_started() {
            self.plugin_audio_processor.as_started_mut()
        } else {
            println!("Starting processor!");
            self.plugin_audio_processor.start_processing()
        }
        .unwrap();

        let audio_inputs = InputAudioBuffers::empty();
        let input_events = self.input_events.as_input();
        let mut output_events = OutputEvents::void();
        let steady_time = None;
        let transport = None;

        let mut audio_outputs =
            self.audio_ports
                .with_output_buffers(out_audio_buffers.iter_mut().map(|port| AudioPortBuffer {
                    latency: 0,
                    channels: AudioPortBufferType::f32_output_only(port.channels_mut()),
                }));

        processor
            .process(
                &audio_inputs,
                &mut audio_outputs,
                &input_events,
                &mut output_events,
                steady_time,
                transport,
            )
            .unwrap();
    }
}

impl ClapPluginProcessor {
    fn update_input_events(&mut self, graph: &Graph, node: &Node, timestamp: &Duration) {
        self.input_events.clear();

        if node.desc.event_input_connections.is_empty() {
            return;
        }

        let InputConnection::Connected(input_node, input_port) =
            node.desc.event_input_connections[0]
        else {
            return;
        };

        let events = &graph.get_node(&input_node).output_event_buffers.get()[input_port];

        for event in events {
            let mut data: [u8; 3] = Default::default();
            event.midi.copy_to_slice(&mut data).unwrap();

            let me = MidiEvent::new(0, 0, data);

            self.input_events.push(&me);
        }
    }
}
