# saw-it

A simple, lightweight CLI tool that watches for file changes in a directory and executes a specified command.

## Philosophy

The name **saw** is the past tense of the verb **see**. The concept is grounded in simple cause and effect:

> "I **saw** a change, so I did something."

This tool acts as a vigilant observer for your workspace. It watches your files, and the moment it *saw* a modification, it immediately triggers your specified action. It bridges the gap between observing an event and automating the response.

## Features

- **Watch:** Recursively watches a directory or file for changes.
- **Execute:** specific command when a change is detected.
- **Cross-platform:** Works on Linux, macOS, and Windows.
- **Clear Screen:** Option to clear the terminal before running the command.

## Usage

```bash
saw --it <PATH> --do <COMMAND>
```

### Options

| Short | Long | Description | Default |
| :--- | :--- | :--- | :--- |
| | `--it` | Directory or file to watch | (Required) |
| | `--do` | Command to execute on change | (Required) |
| `-c` | `--clear` | Clear screen before executing | `false` |
| `-v` | `--verbose` | Print verbose logs (change info, etc.) | `false` |
| `-r` | `--restart` | Terminate and restart if still running | `false` |
| `-h` | `--help` | Print help information | |
| `-V` | `--version` | Print version information | |

### Examples

Watch the current directory and run `cargo check` on change:
```bash
saw --it . --do "cargo check"
```

Watch the `src` folder, clear the screen, and run tests with verbose output:
```bash
saw --it src -c -v --do "cargo test"
```

Watch a specific file and restart the command if it's still running:
```bash
saw --it server.py -r --do "python server.py"
```

Watch a specific file and run a python script:
```bash
saw --it script.py --do "python script.py"
```

## Installation

### From Cargo

```bash
cargo install sawit
```

### From Binstall (Quickest)

```bash
cargo binstall sawit
```

### From Source

```bash
git clone https://github.com/fepfitra/saw-it.git
cd saw-it
cargo install --path .
```

This will install the binary `sawit` and the shorthand `saw`.
