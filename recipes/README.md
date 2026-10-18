# Recipes

Built-in recipes shipped with Metamorphosis. Each TOML file becomes a ring segment for the
files it accepts and can be run with `morph run "<name>" <files…>`.

Your own recipes go in `~/.config/morph/recipes/` (Windows: `%APPDATA%\morph\recipes\`) and
use the same format. Steps are written as `convert:<format>`, `<tool>:<value>` or
`<tool>:key=value,key=value`:

```toml
name    = "Square post"
ring    = "tools"                 # convert | tools | actions
accepts = ["image/*"]             # optional; defaults to what the first step accepts
steps   = ["convert:jpg", "crop:1:1", "bg:preset=soft"]
```
