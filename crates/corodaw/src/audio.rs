use std::{
    sync::mpsc::{Receiver, Sender, channel},
    time::Duration,
};

use crate::audio_graph::AudioGraph;
use anyhow::Error;
use cpal::{
    BufferSize, OutputCallbackInfo, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

enum Message {
    SetGraph(AudioGraph),
}

pub struct Audio {
    _stream: Stream,
    sender: Sender<Message>,
}

impl Audio {
    pub fn new() -> Result<Audio, Error> {
        let cpal = cpal::default_host();
        let device = cpal.default_output_device().unwrap();

        let config = StreamConfig {
            channels: 2,
            sample_rate: 48_000,
            buffer_size: BufferSize::Fixed(1024),
        };

        let (sender, receiver) = channel();

        let mut audio_thread = AudioThread {
            receiver,
            audio_graph: None,
            channels: config.channels,
            sample_rate: config.sample_rate,
        };

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], info| {
                audio_thread.data_callback(data, info);
            },
            |a| println!("error_callback: {:?}", a),
            None,
        )?;

        stream.play()?;

        Ok(Audio {
            _stream: stream,
            sender,
        })
    }

    pub fn set_audio_graph(&self, audio_graph: AudioGraph) -> Result<(), Error> {
        self.sender
            .send(Message::SetGraph(audio_graph))
            .expect("Send should only fail if the receiver was dropped");
        Ok(())
    }
}

struct AudioThread {
    receiver: Receiver<Message>,
    audio_graph: Option<AudioGraph>,
    channels: u16,
    sample_rate: u32,
}

impl AudioThread {
    fn data_callback(&mut self, data: &mut [f32], _info: &OutputCallbackInfo) {
        self.handle_messages();

        if let Some(_audio_graph) = &self.audio_graph {
            // todo!
            data.fill(0.0);
        } else {
            data.fill(0.0);
        }
    }

    fn handle_messages(&mut self) {
        while let Some(message) = self.receiver.try_recv().ok() {
            match message {
                Message::SetGraph(new_graph) => self.audio_graph = Some(new_graph),
            }
        }
    }
}
pub fn t() -> Result<(), Error> {
    let a = Audio::new()?;

    std::thread::sleep(Duration::from_millis(50));
    a.set_audio_graph(AudioGraph::default())?;
    std::thread::sleep(Duration::from_millis(50));

    drop(a);

    Ok(())
}
