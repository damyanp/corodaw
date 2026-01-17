use anyhow::Error;
use cpal::{
    BufferSize, OutputCallbackInfo, Stream, StreamConfig, StreamInstant,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};

use audio_graph::AudioGraphWorker;

pub struct Audio {
    _stream: Stream,
}

impl Audio {
    pub fn new(audio_graph_worker: AudioGraphWorker) -> Result<Audio, Error> {
        let cpal = cpal::default_host();
        let device = cpal.default_output_device().unwrap();

        let config = StreamConfig {
            channels: 2,
            sample_rate: 48_000,
            buffer_size: BufferSize::Fixed(4096),
        };

        //audio_graph_worker.configure(config.channels, config.sample_rate);

        let mut audio_thread = AudioThread {
            audio_graph_worker,
            channels: config.channels,
            _sample_rate: config.sample_rate,
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

        Ok(Audio { _stream: stream })
    }
}

struct AudioThread {
    audio_graph_worker: AudioGraphWorker,
    channels: u16,
    _sample_rate: u32,
    first_playback: Option<StreamInstant>,
}

impl AudioThread {
    fn data_callback(&mut self, data: &mut [f32], info: &OutputCallbackInfo) {
        let playback_time = info.timestamp().playback;
        let first_playback = self.first_playback.get_or_insert(playback_time);

        // Sometimes (or, at least the second time the callback is called) the
        // playback time is before the previous playback time.
        if *first_playback > playback_time {
            *first_playback = playback_time;
        }

        self.audio_graph_worker.tick(
            self.channels,
            data,
            playback_time.duration_since(first_playback).unwrap(),
        );
    }
}
