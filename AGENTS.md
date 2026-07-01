# General use

## Python Coding

- Always "cd" to the folder in question before running any "cargo" command.
- Only use "uv run" and "uv add" subcommands to run any python.
- Blender 5.2 is installed on path, you can also run scripts using subprocess -> blender.


## Rust Coding

- Always "cd" to the folder in question before running any "cargo" command.
- Agent can use "cargo doc" to spawn docs and then browser to open the generated page and look at docs that way.
- Agent can use "cargo build" and "cargo check".
- Agent will never set "CARGO_INCREMENTAL=0" or other custom build parameters, only "cd ... && cargo check" with no other envs.