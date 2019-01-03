MIDI-to-Keypress
================

Takes MIDI events and turns them into keypresses.  This is based on https://github.com/xobs/midi-to-keypress and modified so it can be used with Osu!.

Building
--------

This program requires Rust.  Download it from [rustup.rs](https://rustup.rs).

To and run, go into this directory and type:

````
cargo run
````

Usage
-----

To list available devices, run `miditran --list`.  To specify a device to use as an input, run `miditran --device [device-name]`.

Currently, there is no external configuration. For any MIDI note on messages, a "X" keyboard keypress will be triggered.
