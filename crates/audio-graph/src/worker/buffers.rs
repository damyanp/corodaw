use std::cell::{Ref, RefCell};

use audio_blocks::{AudioBlock, AudioBlockMut, AudioBlockSequential};

use crate::{AgEvent, Event};

pub struct AudioBuffers {
    pub(crate) ports: RefCell<Vec<AudioBlockSequential<f32>>>,
}

impl AudioBuffers {
    pub(crate) fn new(num_ports: u16, num_frames: usize) -> Self {
        AudioBuffers {
            ports: RefCell::new(AudioBuffers::build_ports(num_ports, num_frames)),
        }
    }

    fn build_ports(num_ports: u16, num_frames: usize) -> Vec<AudioBlockSequential<f32>> {
        (0..num_ports)
            .map(|_| AudioBlockSequential::new(1, num_frames))
            .collect()
    }

    pub fn get(&self) -> Ref<'_, Vec<AudioBlockSequential<f32>>> {
        self.ports.borrow()
    }

    pub(crate) fn prepare_for_processing(&self, num_frames: usize) {
        let mut ports = self.ports.borrow_mut();

        if let Some(port) = ports.first()
            && port.num_frames_allocated() < num_frames
        {
            println!("Allocating new audio buffers for {num_frames} frames");
            *ports = AudioBuffers::build_ports(ports.len() as u16, num_frames);
        } else {
            for port in ports.iter_mut() {
                port.set_active_num_frames(num_frames);
            }
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
