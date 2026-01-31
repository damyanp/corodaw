use std::cell::{Ref, RefCell};

use audio_blocks::{AudioBlock, AudioBlockMut, AudioBlockSequential};

use crate::{AgEvent, Event};

pub struct AudioBuffers {
    pub(crate) buffers: RefCell<AudioBlockSequential<f32>>,
}

impl AudioBuffers {
    pub(crate) fn new(num_channels: u16, num_frames: usize) -> Self {
        AudioBuffers {
            buffers: RefCell::new(AudioBlockSequential::new(num_channels, num_frames)),
        }
    }

    pub fn get(&self) -> Ref<'_, AudioBlockSequential<f32>> {
        self.buffers.borrow()
    }

    pub(crate) fn prepare_for_processing(&self, num_frames: usize) {
        let mut buffers = self.buffers.borrow_mut();

        if buffers.num_frames_allocated() < num_frames {
            println!("Allocating new audio buffers for {num_frames} frames");
            *buffers = AudioBlockSequential::new(buffers.num_channels(), num_frames);
        } else {
            buffers.set_num_frames_visible(num_frames);
        }
    }
}

pub struct EventBuffers {
    pub(crate) ports: RefCell<Vec<Vec<AgEvent>>>,
}

impl EventBuffers {
    pub(crate) fn new(num_ports: usize) -> Self {
        EventBuffers {
            ports: RefCell::new((0..num_ports).map(|_| Vec::new()).collect()),
        }
    }

    pub fn get(&self) -> Ref<'_, Vec<Vec<AgEvent>>> {
        self.ports.borrow()
    }

    pub(crate) fn prepare_for_processing(&self) {
        for port in self.ports.borrow_mut().iter_mut() {
            port.clear();
        }
    }
}
