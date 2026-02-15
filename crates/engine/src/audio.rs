use anyhow::Error;
use cpal::{
    BufferSize, OutputCallbackInfo, Stream, StreamConfig, StreamInstant,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

use audio_graph::GraphWorker;

pub struct AudioOutput {
    _stream: Stream,
}

impl AudioOutput {
    pub fn new(mut audio_graph_worker: GraphWorker) -> Result<AudioOutput, Error> {
        let cpal = cpal::host_from_id(cpal::HostId::Asio)?;
        let device = cpal.default_output_device().unwrap();

        let config = StreamConfig {
            channels: 2,
            sample_rate: 48_000,
            buffer_size: BufferSize::Fixed(4096),
        };
        println!("cpal: {:?}", cpal.id());
        println!("Audio device: {:?}", device.description());

        audio_graph_worker.configure(config.channels, config.sample_rate);

        let mut audio_thread = AudioOutputThread {
            audio_graph_worker,
            first_playback: None,
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

        Ok(AudioOutput { _stream: stream })
    }
}

struct AudioOutputThread {
    audio_graph_worker: GraphWorker,
    first_playback: Option<StreamInstant>,
}

impl AudioOutputThread {
    fn data_callback(&mut self, data: &mut [f32], info: &OutputCallbackInfo) {
        let playback_time = info.timestamp().playback;
        let first_playback = self.first_playback.get_or_insert(playback_time);
        // Sometimes (or, at least the second time the callback is called) the
        // playback time is before the previous playback time.
        if *first_playback > playback_time {
            *first_playback = playback_time;
        }

        self.audio_graph_worker
            .tick(data, playback_time.duration_since(first_playback).unwrap());
    }
}
