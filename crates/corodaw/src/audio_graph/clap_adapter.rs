use std::cell::RefCell;

use audio_blocks::AudioBlock;
use clack_host::{
    prelude::{
        AudioPortBuffer, AudioPortBufferType, AudioPorts, InputAudioBuffers, InputEvents,
        OutputEvents,
    },
    process::PluginAudioProcessor,
};

use crate::{
    audio_graph::{AudioPortDesc, NodeDesc, Processor},
    plugins::ClapPlugin,
};

pub fn get_audio_graph_node_desc_for_clap_plugin(
    clap_plugin: &ClapPlugin,
    is_generator: bool,
) -> NodeDesc {
    let collect_ports = |is_input| {
        clap_plugin
            .get_audio_ports(is_input)
            .into_iter()
            .map(|port| AudioPortDesc {
                num_channels: port
                    .try_into()
                    .expect("There should be no more channels than can fit in a u16"),
            })
    };

    NodeDesc {
        _is_generator: is_generator,
        processor: RefCell::new(Box::new(ClapPluginProcessor::new(clap_plugin))),
        audio_inputs: collect_ports(true).collect(),
        audio_outputs: collect_ports(false).collect(),
    }
}

struct ClapPluginProcessor {
    plugin_audio_processor: PluginAudioProcessor<ClapPlugin>,
}

impl ClapPluginProcessor {
    fn new(clap_plugin: &ClapPlugin) -> Self {
        Self {
            plugin_audio_processor: clap_plugin.get_audio_processor(),
        }
    }
}

impl Processor for ClapPluginProcessor {
    fn process(&mut self, out_audio_buffers: &mut [audio_blocks::AudioBlockSequential<f32>]) {
        let processor = if self.plugin_audio_processor.is_started() {
            self.plugin_audio_processor.as_started_mut()
        } else {
            println!("Starting processor!");
            self.plugin_audio_processor.start_processing()
        }
        .unwrap();

        let audio_inputs = InputAudioBuffers::empty();
        let input_events = InputEvents::empty();
        let mut output_events = OutputEvents::void();
        let steady_time = None;
        let transport = None;

        let total_channel_count = out_audio_buffers
            .iter()
            .map(|buffer| buffer.num_channels())
            .reduce(|a, b| a + b)
            .unwrap_or(0);

        let mut audio_ports =
            AudioPorts::with_capacity(total_channel_count as usize, out_audio_buffers.len());

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
