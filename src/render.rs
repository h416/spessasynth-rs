use std::fs;
use std::time::Instant;

use crate::midi::basic_midi::BasicMidi;
use crate::sequencer::sequencer::SpessaSynthSequencer;
use crate::soundbank::sound_bank_loader::load_sound_bank;
use crate::synthesizer::audio_engine::engine_components::synth_constants::DEFAULT_SYNTH_MODE;
use crate::synthesizer::processor::SpessaSynthProcessor;
use crate::synthesizer::types::SynthProcessorOptions;
use crate::utils::{audio_to_wav, WaveWriteOptions};

/// Options for MIDI to WAV rendering.
pub struct RenderOptions {
    pub sample_rate: u32,
    pub gain: f64,
    pub normalize: bool,
    pub buffer_size: usize,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            gain: 1.0,
            normalize: true,
            buffer_size: 128,
        }
    }
}

/// Render MIDI data to WAV bytes.
///
/// Takes raw SoundFont and MIDI byte data, returns WAV file bytes.
/// Optionally accepts render options and a progress callback `(current_time, total_duration)`.
pub fn render_midi_to_wav(
    sf_data: &[u8],
    mid_data: &[u8],
    options: Option<RenderOptions>,
    progress: Option<&dyn Fn(f64, f64)>,
) -> Vec<u8> {
    let opts = options.unwrap_or_default();

    // Parse MIDI
    let midi =
        BasicMidi::from_array_buffer(mid_data, "").expect("Failed to parse MIDI file");

    let sample_count = ((opts.sample_rate as f64) * (midi.duration + 2.0)).ceil() as usize;

    // Create synthesizer
    let synth = SpessaSynthProcessor::new(
        opts.sample_rate as f64,
        |_| {},
        SynthProcessorOptions {
            enable_event_system: false,
            enable_effects: true,
            ..Default::default()
        },
    );

    // Load sound bank
    let mut seq = SpessaSynthSequencer::new(synth);
    seq.synth
        .synth_core
        .sound_bank_manager
        .add_sound_bank(load_sound_bank(sf_data.to_vec()), "main".to_string(), 0);

    // Initialize presets
    seq.synth
        .synth_core
        .reset_all_controllers(DEFAULT_SYNTH_MODE);

    // Load MIDI and play
    seq.load_new_song_list(vec![midi]);
    seq.play();

    // Prepare output buffers
    let mut output_array = vec![vec![0.0f32; sample_count], vec![0.0f32; sample_count]];

    let start = Instant::now();
    let mut filled_samples: usize = 0;
    let mut tick: u64 = 0;
    let duration_rounded =
        (seq.midi_data().map_or(0.0, |m| m.duration) * 100.0).floor() / 100.0;

    while filled_samples < sample_count {
        seq.process_tick();

        let buf_size = opts.buffer_size.min(sample_count - filled_samples);
        seq.synth.render_audio(
            &mut output_array,
            filled_samples,
            buf_size,
        );

        filled_samples += buf_size;
        tick += 1;

        if tick % 1000 == 0 {
            let current_time = (seq.current_time() * 100.0).floor() / 100.0;
            if let Some(ref cb) = progress {
                cb(current_time, duration_rounded);
            }
        }
    }

    let rendered_ms = start.elapsed().as_millis();
    let duration = seq.midi_data().map_or(0.0, |m| m.duration);
    let speed = if rendered_ms > 0 {
        ((duration * 1000.0) / rendered_ms as f64 * 100.0).floor() / 100.0
    } else {
        0.0
    };
    eprintln!("Rendered in {} ms ({}x)", rendered_ms, speed);

    // Apply gain
    if opts.gain != 1.0 {
        for ch in output_array.iter_mut() {
            for s in ch.iter_mut() {
                *s *= opts.gain as f32;
            }
        }
    }

    // Encode WAV
    let (wave, clipped) = audio_to_wav(
        &output_array,
        opts.sample_rate,
        Some(WaveWriteOptions {
            normalize_audio: opts.normalize,
            ..Default::default()
        }),
    );
    if clipped > 0 {
        let total = output_array[0].len() * output_array.len();
        eprintln!(
            "Warning: {} samples clipped ({:.2}% of total). Consider reducing gain or enabling normalization.",
            clipped,
            clipped as f64 / total as f64 * 100.0
        );
    }

    wave
}

/// Render a MIDI file to a WAV file.
///
/// Convenience wrapper around [`render_midi_to_wav`] that handles file I/O.
pub fn render_midi_file_to_wav(
    sf_path: &str,
    mid_path: &str,
    wav_path: &str,
    options: Option<RenderOptions>,
    progress: Option<&dyn Fn(f64, f64)>,
) {
    let sf_data = fs::read(sf_path).expect("Failed to read sound bank file");
    let mid_data = fs::read(mid_path).expect("Failed to read MIDI file");

    let wav_bytes = render_midi_to_wav(&sf_data, &mid_data, options, progress);

    fs::write(wav_path, wav_bytes).expect("Failed to write WAV file");
    eprintln!("File written to {}", wav_path);
}
