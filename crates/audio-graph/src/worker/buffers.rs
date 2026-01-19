use std::cell::{Ref, RefCell};

use audio_blocks::{AudioBlock, AudioBlockMut, AudioBlockSequential};

use crate::Event;

pub struct AudioBuffers {
    pub(crate) channels: RefCell<Vec<AudioBlockSequential<f32>>>,
}

impl AudioBuffers {
    pub(crate) fn new(num_channels: u16, num_frames: usize) -> Self {
        AudioBuffers {
            channels: RefCell::new(AudioBuffers::build_channels(num_channels, num_frames)),
        }
    }

    fn build_channels(num_channels: u16, num_frames: usize) -> Vec<AudioBlockSequential<f32>> {
        (0..num_channels)
            .map(|_| AudioBlockSequential::new(1, num_frames))
            .collect()
    }

    pub fn get(&self) -> Ref<'_, Vec<AudioBlockSequential<f32>>> {
        self.channels.borrow()
    }

    pub(crate) fn prepare_for_processing(&self, num_frames: usize) {
        let mut channels = self.channels.borrow_mut();

        if let Some(channel) = channels.first()
            && channel.num_frames_allocated() < num_frames
        {
            println!("Allocating new audio buffers for {num_frames} frames");
            *channels = AudioBuffers::build_channels(channels.len() as u16, num_frames);
        } else {
            for channel in channels.iter_mut() {
                channel.set_active_num_frames(num_frames);
            }
        }
    }
}

pub struct EventBuffers {
    pub(crate) ports: RefCell<Vec<Vec<Event>>>,
}

impl EventBuffers {
    pub(crate) fn new(num_ports: usize) -> Self {
        EventBuffers {
            ports: RefCell::new((0..num_ports).map(|_| Vec::new()).collect()),
        }
    }

    pub fn get(&self) -> Ref<'_, Vec<Vec<Event>>> {
        self.ports.borrow()
    }

    pub(crate) fn prepare_for_processing(&self) {
        for port in self.ports.borrow_mut().iter_mut() {
            port.clear();
        }
    }
}
