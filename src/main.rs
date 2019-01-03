#![windows_subsystem = "console"]

extern crate clap;
extern crate enigo;
extern crate midir;

use std::error::Error;
use std::time::Duration;
use std::thread;
use std::fmt::Write;
use std::collections::HashMap;

use clap::{App, Arg};

use midir::{Ignore, MidiInput, MidiInputConnection};

mod midi;
use midi::{MidiMessage, MidiEvent, MidiNote};

mod appstate;
use appstate::AppState;

mod notemappings;
use notemappings::{Event, KbdKey, NoteMapping, NoteMappings};

/// The amount of time to wait for a keyboard modifier to stick
const MOD_DELAY_MS: u64 = 150;

/// The amount of time to wait for a keydown event to stick
const KEY_DELAY_MS: u64 = 40;

/// The amount of time required for system events, such as Esc
const SYS_DELAY_MS: u64 = 400;

/// A small delay required when switching between octaves.
const OCTAVE_DELAY_MS: u64 = 10;

fn main() {
    let matches = App::new("Midi Perform")
        .version("0.2.9")
        .author("Sean Cross <sean@xobs.io>")
        .about("Accepts MIDI controller data and simulates keyboard presses")
        .arg(
            Arg::with_name("list")
                .short("l")
                .long("list")
                .help("List available devices"),
        )
        .arg(
            Arg::with_name("device")
                .short("d")
                .long("device")
                .help("Connect to specified device")
                .value_name("DEVICE"),
        )
        .get_matches();

    if matches.is_present("list") {
        list_devices().expect("unable to list MIDI devices");
        return;
    }

    let device_name = match matches.value_of("device") {
        Some(s) => Some(s.to_owned()),
        None => None,
    };
    run(device_name).unwrap();
}

/// This function is called for every message that gets passed in.
fn midi_callback(_timestamp_us: u64, raw_message: &[u8], app_state: &AppState) {
    let mut keygen = app_state.keygen().lock().unwrap();
 
    if let Ok(msg) = MidiMessage::new(raw_message) {
        if let Some(note_mapping) = app_state.mappings().lock().unwrap().find(msg.note(), msg.channel(), None) {
            let sequence = match msg.event() {
                &MidiEvent::NoteOn => &note_mapping.on,
                &MidiEvent::NoteOff => &note_mapping.off,
            };

            //println!("Found note mapping: {:?} for event {:?}, running sequence {:?}", note_mapping, msg.event(), sequence);
            for event in sequence {
                match event {
                    &notemappings::Event::Delay(msecs) => thread::sleep(Duration::from_millis(msecs)),
                    &notemappings::Event::KeyDown(ref k) => {keygen.key_down(&k);},
                    &notemappings::Event::KeyUp(ref k) => {keygen.key_up(&k);},

                    // For NoteMod, which goes at the top of a note, see if we need to change
                    // the current set of modifiers.  If so, pause a short while.
                    // This enables fast switching between notes in the same octave, where no
                    // keychange is required.
                    &notemappings::Event::NoteMod(ref kopt) => {
                        let mut changes = 0;
                        let key_mods = vec![KbdKey::Shift, KbdKey::Control];
                        if let &Some(ref k) = kopt {
                            for key_mod in key_mods {
                                if &key_mod == k {
                                    if keygen.key_down(&key_mod) {
                                        changes = changes + 1;
                                    }
                                }
                                else {
                                    if keygen.key_up(&key_mod) {
                                        changes = changes + 1;
                                    }
                                }
                            }
                        }
                        else {
                            for key_mod in key_mods {
                                if keygen.key_up(&key_mod) {
                                    changes = changes + 1;
                                }
                            }
                        }
                        if changes > 0 {
                            thread::sleep(Duration::from_millis(OCTAVE_DELAY_MS));
                        }
                    },
                }
            }
        }
        else {
            println!("No note mapping for {:?} @ {:?}", msg.note(), msg.channel()); 
        }
    }

    /*
    let mut s = String::new();
    for &byte in raw_message {
        write!(&mut s, "{:X} ", byte).expect("Unable to write");
    }
    println!("Unhandled message for data: {}", s);
    */
}

fn generate_old_mappings(mappings: &mut NoteMappings) {
    // just map a bunch of keys to 'x'
    for idx in 1..96 {
        let base = MidiNote::C0.index();
        let mut note_mapping_mid = NoteMapping::new(MidiNote::new(idx as u8 + base).expect("Invalid note index"), 0, None);


        note_mapping_mid.on = NoteMapping::down_event('x', None, None);
        note_mapping_mid.off = NoteMapping::up_event('x', None, None);

        mappings.add(note_mapping_mid);
    }
}


fn run(midi_name: Option<String>) -> Result<(), Box<Error>> {
    let mut midi_ports: HashMap<String, MidiInputConnection<()>> = HashMap::new();
    let app_state = AppState::new();

    //app_state.mappings().lock().unwrap().import("note_mappings.txt").ok();
    generate_old_mappings(&mut app_state.mappings().lock().unwrap());

    loop {
        let port_count = MidiInput::new("perform-count").expect("Couldn't create midi input").port_count();

        let mut seen_names: HashMap<String, bool> = HashMap::new();

        // Look through all available ports, and see if the name already has
        // a corresponding closure in the callback table.
        for idx in 0..port_count {
            let mut midi_in = MidiInput::new("perform").expect("Couldn't create performance input");
            match midi_in.port_name(idx) {
                Err(_) => (),
                Ok(name) => {
                    seen_names.insert(name.clone(), true);
                    // We have a name now.  See if it's in the closure table.
                    if midi_ports.contains_key(&name) {
                        continue;
                    }

                    // If we're looking for a particular device, return if it's not the one we've found.
                    if let Some(ref target_name) = midi_name {
                        if target_name != &name {
                            continue;
                        }
                    }

                    // This device is new.
                    midi_in.ignore(Ignore::None);
                    let app_state_thr = app_state.clone();
                    match midi_in.connect(
                        idx,
                        "key monitor",
                        move |ts, raw_msg, _ignored| {
                            midi_callback(ts, raw_msg, &app_state_thr);
                        },
                        (),
                    ) {
                        Err(reason) => println!("Unable to connect to device: {:?}", reason),
                        Ok(conn) => {
                            println!("Connection established to {}", name);
                            midi_ports.insert(name, conn);
                        }
                    }
                }
            }
        }

        let mut to_delete = vec![];
        for name in midi_ports.keys() {
            if ! seen_names.contains_key(name) {
                to_delete.push(name.clone());
            }
        }
        for name in to_delete {
            println!("Disconnected from {}", name);
            midi_ports.remove(&name);
        }
        thread::sleep(Duration::from_secs(1));
    }
}

fn list_devices() -> Result<(), Box<Error>> {
    let mut midi_in = MidiInput::new("perform")?;
    midi_in.ignore(Ignore::None);

    println!("Available MIDI devices:");
    for i in 0..midi_in.port_count() {
        println!("    {}", midi_in.port_name(i)?);
    }

    Ok(())
}
