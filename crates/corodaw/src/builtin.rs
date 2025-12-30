use std::{
    cell::{Ref, RefCell, RefMut},
    sync::mpsc::{Receiver, Sender, channel},
};

use audio_blocks::{AudioBlock, AudioBlockMut, AudioBlockOps, AudioBlockSequential};

use crate::audio_graph::{AudioPortDesc, NodeDesc, Processor};

#[derive(Default)]
pub struct GainControl {
    gain: f32,
    sender: Option<Sender<f32>>,
}

impl GainControl {
    pub fn get_node_desc(&mut self) -> NodeDesc {
        assert!(self.sender.is_none());

        let (sender, receiver) = channel();
        self.sender = Some(sender);

        let processor = GainControlProcessor {
            receiver,
            gain: self.gain,
        };

        NodeDesc {
            _is_generator: false,
            processor: RefCell::new(Box::new(processor)),
            audio_inputs: vec![AudioPortDesc { num_channels: 2 }],
            audio_outputs: vec![AudioPortDesc { num_channels: 2 }],
        }
    }

    pub fn set_gain(&mut self, gain: f32) {
        self.gain = gain;
        if let Some(sender) = &self.sender {
            sender.send(gain).unwrap();
        }
    }
}

struct GainControlProcessor {
    receiver: Receiver<f32>,
    gain: f32,
}

impl Processor for GainControlProcessor {
    fn process(
        &mut self,
        in_audio_buffers: &[Option<Ref<'_, AudioBlockSequential<f32>>>],
        out_audio_buffers: &mut [RefMut<'_, AudioBlockSequential<f32>>],
    ) {
        self.process_messages();

        for (input, output) in in_audio_buffers.iter().zip(out_audio_buffers.iter_mut()) {
            if let Some(input) = input {
                for (input, output) in input.channels_iter().zip(output.channels_iter_mut()) {
                    for (input, output) in input.zip(output) {
                        *output = input * self.gain;
                    }
                }
            } else {
                output.fill_with(0.0);
            }
        }
    }
}

impl GainControlProcessor {
    fn process_messages(&mut self) {
        if let Ok(gain) = self.receiver.try_recv() {
            self.gain = gain;
        }
    }
}
