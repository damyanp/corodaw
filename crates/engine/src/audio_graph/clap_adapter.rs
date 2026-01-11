use std::{
    cell::{RefCell, RefMut},
    fmt::Debug,
};

use audio_blocks::AudioBlockSequential;
use clack_host::{
    prelude::{
        AudioPortBuffer, AudioPortBufferType, AudioPorts, InputAudioBuffers, InputEvents,
        OutputEvents,
    },
    process::PluginAudioProcessor,
};

use crate::{
    audio_graph::{AudioGraph, Graph, Node, NodeCreator, NodeId, Processor},
    plugins::ClapPlugin,
};

impl NodeCreator for ClapPlugin {
    fn create_node(&self, graph: &AudioGraph) -> NodeId {
        let count_ports = |is_input| {
            self.get_audio_ports(is_input)
                .into_iter()
                .map(|port| port)
                .reduce(|a, b| a + b)
                .unwrap_or(0)
        };

        let audio_inputs = count_ports(true);
        let audio_outputs = count_ports(false);

        graph.add_node(
            audio_inputs as usize,
            audio_outputs as usize,
            Box::new(ClapPluginProcessor::new(self)),
        )
    }
}

pub struct ClapPluginProcessor {
    plugin_audio_processor: RefCell<PluginAudioProcessor<ClapPlugin>>,
    audio_ports: RefCell<AudioPorts>,
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
            plugin_audio_processor: RefCell::new(clap_plugin.get_audio_processor()),
            audio_ports: RefCell::new(audio_ports),
            num_outputs: total_channel_count,
        }
    }

    pub fn get_total_output_channels(&self) -> usize {
        self.num_outputs
    }
}

impl Processor for ClapPluginProcessor {
    fn process(
        &self,
        _: &Graph,
        _: &Node,
        out_audio_buffers: &mut [RefMut<'_, AudioBlockSequential<f32>>],
    ) {
        let mut processor = self.plugin_audio_processor.borrow_mut();

        let processor = if processor.is_started() {
            processor.as_started_mut()
        } else {
            println!("Starting processor!");
            processor.start_processing()
        }
        .unwrap();

        let audio_inputs = InputAudioBuffers::empty();
        let input_events = InputEvents::empty();
        let mut output_events = OutputEvents::void();
        let steady_time = None;
        let transport = None;

        let mut audio_ports = self.audio_ports.borrow_mut();

        let mut audio_outputs =
            audio_ports.with_output_buffers(out_audio_buffers.iter_mut().map(|port| {
                AudioPortBuffer {
                    latency: 0,
                    channels: AudioPortBufferType::f32_output_only(port.channels_mut()),
                }
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
