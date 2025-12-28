use std::{rc::Rc, sync::Arc};

use clack_extensions::audio_ports::PluginAudioPorts;
use clack_host::process::{PluginAudioProcessor, StoppedPluginAudioProcessor};
use cpal::SampleFormat;
use gpui::SharedString;

use crate::plugins::ClapPlugin;

#[derive(Default)]
pub struct AudioGraph {
    pub nodes: Vec<Node>,
}

struct Node {
    contents: Box<dyn NodeContents + Send>,
    // audio_inputs: Vec<AudioPort>,
    // audio_outputs: Vec<AudioPort>,
}

pub trait NodeContents {
    fn get_audio_inputs(&self) -> Vec<AudioPortDesc>;
    fn get_audio_outputs(&self) -> Vec<AudioPortDesc>;
}

impl NodeContents for PluginAudioProcessor<ClapPlugin> {
    fn get_audio_inputs(&self) -> Vec<AudioPortDesc> {
        todo!()
    }

    fn get_audio_outputs(&self) -> Vec<AudioPortDesc> {
        todo!()
    }
}

fn test(x: PluginAudioProcessor<ClapPlugin>) -> Node {
    Node {
        contents: Box::new(x),
    }
}

pub struct AudioPortDesc {
    name: SharedString,
    channel_count: u32,
    sample_format: SampleFormat,
}

struct AudioPort {
    node: Arc<Node>,
    name: SharedString,
    destination: Arc<AudioPort>,
}

struct Fader {
    gain: f32,
}

struct Mixer;
