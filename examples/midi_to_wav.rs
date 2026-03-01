use std::env;
use std::fs;

use spessasynth_rs::midi::midi_tools::midi_builder::{MidiBuilder, MidiBuilderOptions};
use spessasynth_rs::midi::midi_tools::midi_writer::write_midi_internal;
use spessasynth_rs::{render_midi_file_to_wav, RenderOptions};

/// Generate a simple test MIDI file: single note C4 (note 60), 1 second, Piano.
fn generate_test_midi(output_path: &str) {
    let mut builder = MidiBuilder::new(MidiBuilderOptions {
        time_division: 480,
        initial_tempo: 120.0, // 120 BPM → 1 beat = 0.5s
        ..MidiBuilderOptions::default()
    })
    .expect("Failed to create MidiBuilder");

    // Program Change: channel 0, program 0 (Acoustic Grand Piano)
    builder.add_program_change(0, 0, 0, 0).unwrap();
    // Note On: tick 0, track 0, channel 0, note C4 (60), velocity 100
    builder.add_note_on(0, 0, 0, 60, 100).unwrap();
    // Note Off: tick 960 (= 2 beats = 1 second at 120 BPM), track 0, channel 0, note 60
    builder.add_note_off(960, 0, 0, 60, 64).unwrap();

    builder.midi.flush(true);

    let bytes = write_midi_internal(&builder.midi);
    fs::write(output_path, &bytes).expect("Failed to write test MIDI file");
    println!("Test MIDI written to {} ({} bytes)", output_path, bytes.len());
    println!("  Duration: {} sec", builder.midi.duration);
}

fn print_usage() {
    println!("Usage: midi_to_wav [OPTIONS] <soundbank> <midi> <wav>");
    println!("       midi_to_wav --gen-test-midi <output.mid>");
    println!();
    println!("OPTIONS:");
    println!("  -g, --gain <value>   Set master gain (default: 1.0)");
    println!("  --no-normalize       Disable audio normalization");
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.first().map(|s| s.as_str()) == Some("--gen-test-midi") {
        if args.len() != 2 {
            println!("Usage: midi_to_wav --gen-test-midi <output.mid>");
            std::process::exit(1);
        }
        generate_test_midi(&args[1]);
        return;
    }

    // Parse options
    let mut gain: f64 = 1.0;
    let mut normalize: bool = true;
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-g" | "--gain" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("Error: --gain requires a value");
                    print_usage();
                    std::process::exit(1);
                }
                gain = args[i].parse::<f64>().unwrap_or_else(|_| {
                    eprintln!("Error: invalid gain value '{}'", args[i]);
                    std::process::exit(1);
                });
                if gain <= 0.0 {
                    eprintln!("Error: gain must be > 0.0");
                    std::process::exit(1);
                }
            }
            "--no-normalize" => {
                normalize = false;
            }
            "-h" | "--help" => {
                print_usage();
                std::process::exit(0);
            }
            _ => {
                positional.push(args[i].clone());
            }
        }
        i += 1;
    }

    if positional.len() != 3 {
        print_usage();
        std::process::exit(1);
    }

    render_midi_file_to_wav(
        &positional[0],
        &positional[1],
        &positional[2],
        Some(RenderOptions {
            gain,
            normalize,
            ..Default::default()
        }),
        Some(&|current, total| {
            println!("Rendered {} / {}", current, total);
        }),
    );
}
