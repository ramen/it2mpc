# it2mpc

Convert [Impulse Tracker](https://en.wikipedia.org/wiki/Impulse_Tracker) (`.IT`) files into Akai MPC projects.

Extracts every sample or instrument from an `.IT` file, writes each one as a `.wav`, and generates the MPC Drum Program / project files (`.xpm`, `.xal`, `.xpj`) ready to open in MPC software.

## Usage

```
it2mpc <INPUT> <OUTPUT> [OPTIONS]
```

| Argument | Description |
|---|---|
| `<INPUT>` | Path to the source `.IT` file |
| `<OUTPUT>` | Output directory (created if it doesn't exist) |

### Options

| Flag | Description |
|---|---|
| `-n`, `--name <NAME>` | Project / program name (defaults to the `.IT` filename stem) |
| `--samples-only` | Ignore instrument headers and export raw samples instead |
| `--info` | Print a summary of the `.IT` file without writing any output |

### Examples

```sh
# Basic conversion
it2mpc song.it ./my_project

# Override the project name
it2mpc song.it ./my_project --name "My Drum Kit"

# Export raw samples even if the file contains instrument definitions
it2mpc song.it ./my_project --samples-only

# Inspect the IT file without writing anything
it2mpc song.it ./my_project --info
```

## Output layout

```
<OUTPUT>/
  <NAME>.xpj                        ← MPC project file
  <NAME>_[ProjectData]/
    <NAME>.xpm                      ← MPC Drum Program
    All Sequences & Songs.xal       ← MPC sequence/song list
    <sample1>.wav
    <sample2>.wav
    ...
```

Open `<NAME>.xpj` in MPC software to load the project.

## Conversion details

- **Instrument mode** (default): one pad per IT instrument. The primary sample is determined by scanning the instrument's note-sample table. Instrument-level properties (volume, panning, tuning, looping, root note, NNA, fadeout) are preserved.
- **Sample mode** (`--samples-only`): one pad per raw IT sample. Used automatically when the file has no instrument headers, or forced with `--samples-only`.
- MPC supports a maximum of **128 pads**; a warning is printed if the source file exceeds this limit.

## Building

Requires [Rust](https://rustup.rs/) 2024 edition or later.

```sh
cargo build --release
```

The compiled binary will be at `target/release/it2mpc`.

## License

See [LICENSE](LICENSE).
