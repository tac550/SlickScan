# SlickScan

This is an experimental desktop document scanning tool (implemented as a SANE scanner frontend).

The goal is to allow very fast scanning, organization, and saving of multiple documents which were placed at-once into a scanner with a document feeder.

The interface allows users to group pages into documents and save them as additional pages continue to be processed by the scanner.

## Compatibility

Currently only supports Linux.
Scanner must be SANE-compatible.

Works best with scanners that use an automatic feeder.

## Building

Requires that libclang be installed prior to building.
On Linux, additional packages are required for the `eframe` egui framework.
Also requires SANE headers.

On Ubuntu: `sudo apt install libclang-dev libsane1 libsane-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libspeechd-dev libxkbcommon-dev libssl-dev libfontconfig1-dev`

## Testing

The program may be tested without a physical scanner using the built-in test backends that come with SANE.
To enable these backends, you must uncomment the `test` line in the file `/etc/sane.d/dll.conf`

To show log output, run with `RUST_LOG=debug cargo run` or similar.

The program may fail to show system dialogs and popups if run from the terminal inside VSCode.
This can be fixed by running `unset GTK_PATH` before running the program. This only needs to be done once per terminal session.
Alternatively, you can use an external terminal emulator instead.
