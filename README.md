# Roboarchive

This is an experimental desktop document scanning tool (implemented as a SANE scanner frontend).

The goal is to allow very fast scanning of multiple documents which were placed at-once into a scanner with a feeder system.

The interface should allow users to group pages into documents and save and organize them as pages continue to be processed by the scanner.

## Compatibility

Currently only supports Linux.
Scanner must be SANE-compatible.

Works best with scanners that use an automatic feeder.

Only supports using a wired connection to the scanner.

## Testing

I'm supporting testing using the built-in test backends that come with SANE.
To enable these backends, you must uncomment the `test` line in the file `/etc/sane.d/dll.conf`

For debugging, run with `RUST_LOG=debug cargo run` or similar.

## Building

Requires that libclang be installed prior to building.
On Linux, additional packages are required for the `eframe` egui framework.
Also requires SANE headers.

On Ubuntu: `sudo apt install libclang-dev libsane-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libspeechd-dev libxkbcommon-dev libssl-dev libfontconfig1-dev`