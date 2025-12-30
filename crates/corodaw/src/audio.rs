use crate::audio_graph::AudioGraphWorker;
use anyhow::Error;
use cpal::{
    BufferSize, OutputCallbackInfo, Stream, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

pub struct Audio {
    _stream: Stream,
}

impl Audio {
    pub fn new(mut audio_graph_worker: AudioGraphWorker) -> Result<Audio, Error> {
        let cpal = cpal::default_host();
        let device = cpal.default_output_device().unwrap();

        let config = StreamConfig {
            channels: 2,
            sample_rate: 48_000,
            buffer_size: BufferSize::Fixed(4096),
        };

        audio_graph_worker.configure(config.channels, config.sample_rate);

        let mut audio_thread = AudioThread {
            audio_graph_worker,
            _channels: config.channels,
            _sample_rate: config.sample_rate,
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

        Ok(Audio { _stream: stream })
    }
}

struct AudioThread {
    audio_graph_worker: AudioGraphWorker,
    _channels: u16,
    _sample_rate: u32,
}

impl AudioThread {
    fn data_callback(&mut self, data: &mut [f32], _info: &OutputCallbackInfo) {
        self.audio_graph_worker.process(data);
    }
}
