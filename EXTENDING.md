# Extending Metamorphosis

There are three ways to add something to the ring, from no code to Rust:

| | What | Where | Restart? |
|---|---|---|---|
| **Chain recipe** | A saved sequence of existing operations | a `.toml` file in your recipes folder | no |
| **Command recipe** | Any local command (`magick`, `ffmpeg`, a script) | a `.toml` file in your recipes folder | no |
| **Engine operation** | Native Rust code implementing `Operation` | a module in `crates/morph-engines` | rebuild |

The recipes folder is `~/.config/morph/recipes/` on Linux and `%APPDATA%\morph\recipes\`
on Windows (or `config\recipes` next to the app in portable mode). The app watches it:
a file you add, edit or delete shows up in (or leaves) the ring on its next open. The
CLI reads it on every run (`morph run "<name>" files…`).

## Chain recipes

```toml
name    = "Square post"
ring    = "tools"                 # convert | tools | actions
accepts = ["image/*"]             # optional; defaults to what the first step accepts
steps   = ["convert:jpg", "crop:1:1", "bg:preset=soft"]
```

Steps are `convert:<format>`, `<tool>:<value>` (the tool's first parameter) or
`<tool>:key=value,key=value`, or a plain operation id such as `image.strip`. Explicit
steps work too:

```toml
[[step]]
op = "image.compress"
params = { target = "1MB" }
```

Job history can save any drag-chain as a recipe ("Save as recipe").

## Command recipes

```toml
name    = "Thumbnail"
ring    = "tools"
accepts = ["image/*"]             # MIME globs, `ext:png`, `family:video`, `folder` or `*`
output  = "{stem} Thumb.webp"
run     = ["magick", "{input}", "-thumbnail", "{size}x{size}", "{output}"]

[[param]]                          # optional; shown as a form in expanded mode
key     = "size"
label   = "Size"
type    = "int"                    # bool | int | float | text | choice
min     = 16
max     = 4096
default = 512
```

`run` is an argument list, not a shell string. Nothing is passed through a shell
unless you ask for one (`["sh", "-c", "…"]`, `["cmd", "/c", "…"]`). The variables are:

| Variable | Value |
|---|---|
| `{input}` | full path of the input file |
| `{output}` | full path the command must write (required) |
| `{stem}`, `{ext}` | input name without extension, and its extension |
| `{dir}` | the input's folder (read-only for the command) |
| `{date}` | today, `YYYY-MM-DD` |
| `{<param>}` | each `[[param]]` value |

`output` is a file name template, never a path. The output's extension decides what
the ring shows (`→ Photo Thumb.webp`).

**Confinement.** The command runs in a fresh staging folder, and `{output}` points
inside it. It may write only there:

- **Windows:** the process gets a low-integrity token, and the staging folder is labelled
  low integrity. Mandatory integrity control blocks writes anywhere else.
- **Linux:** a Landlock ruleset (kernel 5.13 or later) allows writes only beneath the
  staging folder and `/dev`. On older kernels the command runs unconfined, and the log says so.

`TEMP`/`TMP`/`TMPDIR` point at the staging folder. When the command exits with code 0,
`{output}` is committed like any other result: next to the input, with a collision-free
name, never replacing a file. Anything else the command wrote is discarded. On failure,
the ring shows the first line of its stderr, and **DETAILS** shows the rest.

## Watch folders

Settings → **Watch folders**: pick a folder and an operation or recipe. Every new file
in the folder (not in subfolders) runs through it once the file stops growing. Hidden
files, partial downloads (`.part`, `.crdownload`, `.tmp`) and the results subfolder
(default `Metamorphosis`) are skipped, so outputs are never processed twice. Jobs are
recorded in the history, so they can be undone.

## Engine operations (Rust)

An engine operation implements [`morph_core::Operation`](crates/morph-core/src/op.rs):

```rust
pub trait Operation: Send + Sync {
    fn spec(&self) -> &OpSpec;
    fn available(&self) -> bool { true }      // false → "INSTALL COMPONENT" in the ring
    fn run(&self, ctx: &OpContext) -> anyhow::Result<Vec<PathBuf>>;
}
```

The `OpSpec` is everything the UI and the CLI need:

- `OpSpec::convert(ext, label, accepts)`: a conversion (`convert.<ext>`) in the Convert ring.
- `OpSpec::tool(id, label, suffix, accepts, output)`: a tool. Outputs are named
  `{stem} {suffix}.{ext}` by default.
- `OpSpec::action(id, label, accepts)`: an action on the files themselves, never undoable.
- Builders:
  - `.param(ParamSpec)`: becomes a form control and a CLI flag `--key`.
  - `.arity(Arity::All)`: one job for the whole selection, e.g. merge.
  - `.editor("crop")`: opens an expanded-mode editor instead of running directly.
  - `.component("ml")`: needs an optional component.
  - `.order(n)`: sets the position in the ring.

In `run`, read `ctx.inputs` / `ctx.kinds` / `ctx.params`. Report progress with
`ctx.progress.message(fraction, text)` and check `ctx.progress.is_cancelled()` in long
loops. Write only through `ctx.sink`:

- `ctx.sink.file(input, ext)`: a pending file named by the naming template.
- `file_named(dir, name)` / `folder(input)`: a pending file with an exact name, or a pending folder.
- `.commit()`: gives the result its final, collision-free name.

A pending output that is dropped without `commit` is deleted, so a failure never
leaves half a file behind. Never open an input for writing.

The trait's documentation has a complete, tested example (`cargo test -p morph-core --doc`).
Register the operation in its module's `register(reg: &mut Registry)` and call that from
`morph_engines::registry()`. Then add a test under `crates/morph-engines/tests/` that runs
it through `Runner`, the same path the ring and the CLI use.

Rules for engine crates:

- **No network.** `scripts/offline_guard.py` fails CI if a crate other than
  `morph-updater` pulls in network code.
- **Optional heavy dependencies** (models, FFmpeg, LibreOffice) are *components*. Look them up
  with `morph_core::components::find_tool` and return `available() == false` when missing.
- **Non-destructive.** Outputs go next to the input (or the job's `out_dir`), and
  existing files are never replaced.
