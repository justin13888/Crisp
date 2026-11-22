# Authoring with an LLM: `crisp-mcp`

`crisp-mcp` is a [Model Context Protocol](https://modelcontextprotocol.io)
server. It lets an LLM client (Claude Code, Claude Desktop, or any MCP client)
write, edit, store, and publish Crisp recipes for you.

A `.crisp` file is canonical CBOR with derived ids, revision hashes, and a
post-quantum signature, and a model cannot produce one reliably. So the model
writes a recipe as plain JSON, and `crisp-mcp` turns it into a signed file
using `crisp-core`, the same engine the web app and server use. Your key stays
in the `crisp-mcp` process; the model never sees it.

The research method lives in a separate repository: skills such as
`build-recipe` and `edit-recipe` in
[`crisp-skills`](https://github.com/justin13888/crisp-skills) tell the model
how to research a dish, separate verifiable facts from preference, and cite
its sources. Install both.

## Install

```bash
cargo install --path mcp      # from a checkout of this repository
```

This puts the `crisp-mcp` binary in `~/.cargo/bin`.

## Configure

`crisp-mcp` reads its configuration from the environment:

| Variable | Needed for | Meaning |
| --- | --- | --- |
| `CRISP_KEY_FILE` | saving, publishing | Your identity backup from the web app (*Settings → Identity → download the backup*, `crisp-identity.crispkey`) |
| `CRISP_KEY_PASSPHRASE_FILE` | saving, publishing | A file holding that backup's passphrase (preferred) |
| `CRISP_KEY_PASSPHRASE` | saving, publishing | The passphrase itself, if you cannot use a file |
| `CRISP_DISPLAY_NAME` | saving | The name recipes are signed under |
| `CRISP_LIBRARY_DIR` | — | Where `.crisp` files are stored (default `$XDG_DATA_HOME/crisp/library`, else `~/.local/share/crisp/library`) |
| `CRISP_SERVER_URL` | publishing, searching | A Crisp server, e.g. `https://crisp.example` |
| `CRISP_SERVER_USERNAME` | publishing | The username your identity is registered under on that server |
| `CRISP_LOG` / `CRISP_LOG_FORMAT` | — | Log filter (default `info`) / `json` for JSON lines; logs go to stderr |

Nothing is required to start. Without a key, the model can still validate
drafts and read your library, and `crisp_status` says what is missing.

Every request to the server (signing in, publishing, downloading, and
searching) must finish within 2 minutes, from connecting until the last byte
of the answer arrives. A response may be at most 64 MiB, the format's limit
on a whole `.crisp` file. A server that stalls past the timeout, or sends
more than that, fails the tool call with an error that names the operation
and the limit. An oversized answer is not read past the limit.

Keep the key file and the passphrase file readable only by you. On Unix,
`crisp-mcp` refuses to unlock the key when either file grants its group or
other users any read or write permission, and the error names the file and
its mode:

```bash
chmod 600 "$HOME/.config/crisp/crisp-identity.crispkey" "$HOME/.config/crisp/passphrase"
```

The mode is checked on the opened file, so a symlink is judged by the file it
points to. An inline `CRISP_KEY_PASSPHRASE` has no file to check. Signing
recipes as you is exactly what this key is for, so treat any client you give
`crisp-mcp` to as acting in your name.

### Claude Code

```bash
claude mcp add crisp \
  --env CRISP_KEY_FILE="$HOME/.config/crisp/crisp-identity.crispkey" \
  --env CRISP_KEY_PASSPHRASE_FILE="$HOME/.config/crisp/passphrase" \
  --env CRISP_DISPLAY_NAME="Your Name" \
  -- crisp-mcp
```

### Claude Desktop and other clients

```json
{
  "mcpServers": {
    "crisp": {
      "command": "crisp-mcp",
      "env": {
        "CRISP_KEY_FILE": "/home/you/.config/crisp/crisp-identity.crispkey",
        "CRISP_KEY_PASSPHRASE_FILE": "/home/you/.config/crisp/passphrase",
        "CRISP_DISPLAY_NAME": "Your Name",
        "CRISP_SERVER_URL": "https://crisp.example",
        "CRISP_SERVER_USERNAME": "you"
      }
    }
  }
}
```

## Tools

The tool names are a contract with `crisp-skills`. Rename a tool only together
with the skills that call it.

| Tool | What it does |
| --- | --- |
| `crisp_status` | Reports the signing identity, the library, and the server, plus the course, diet, and allergen codes and unit words a draft may use. The unit words are every spelling crisp-core's unit catalogue converts, so they track the catalogue |
| `validate_recipe` | Checks a draft without saving it. Returns every problem with a JSON Pointer into the draft |
| `save_recipe` | Signs a draft and stores it in the library. With `based_on`, the save is a revision of your own recipe or a fork of someone else's, which credits the original |
| `list_recipes` | Lists the library, optionally filtered by a query |
| `get_recipe` | Reads a recipe as an editable draft, from the library or else the server, with notices about anything the draft does not show |
| `publish_recipe` | Uploads a library recipe to the server and stores the endorsed copy it returns |
| `search_server` | Searches recipes already published on the server |

A draft is a JSON object whose field names follow the format: `title`,
`yield`, `ingredients`, `steps`, `notes`, `attribution`, and so on.
Values are written the way the web app's editor accepts them:

- **Amounts** such as `1 1/2 cups`, `200 g`, `2-3 tbsp`, or `to taste`.
- **Durations** such as `1 h 30 min`.
- **Temperatures** such as `220 °C`.
- **Prose**: blank lines separate paragraphs, and `[text](https://…)` makes a
  link.

The tool schemas describe every field.

### What a draft cannot express

Editing goes `get_recipe` → change the draft → `save_recipe` with `based_on`.

**Carried over unchanged:**
- photos, the cover photo, nutrition, and recipe-level extension data;
- per-ingredient, per-step, and per-equipment photos, extension data, and
  component-recipe links, matched by `ref`, plus ingredient-index codes.
  Keep every `ref` as `get_recipe` returns it and give new ingredients and
  equipment unused ones; a step without a `ref` is new. An ingredient or
  equipment item keeps its base data only while its name is unchanged, so a
  renumbered or renamed item starts without it.

Pass the same `based_on` to `validate_recipe` that you will pass to
`save_recipe`, so the check runs against the same base. Also pass the
`revision` that `get_recipe` returned as `expected_revision`. `save_recipe`
then refuses to save over a revision made after you read the recipe, instead
of silently dropping it.

Run one `crisp-mcp` process per library directory. Saves within one process
are serialized, but two processes writing the same `CRISP_LIBRARY_DIR` (for
example, Claude Code and Claude Desktop at once) are not coordinated with
each other.

**Not carried over:**
- **Extension data on substitutes and timers.**
- **Other languages.** A draft is written in one language, and saving keeps
  only that language.
- **Formatting.** Bold, italic, and internal references are saved as plain
  text.

`get_recipe` lists each of these in its `notices` before anything is saved.

## Provenance

Researched recipes record where they came from using fields the format
already has:

| Field | Holds |
| --- | --- |
| `source` | The primary reference the recipe follows |
| `attribution` | Every source relied on, as https links, and a note that the recipe was drafted with AI-assisted research |
| `notes` | The dish's goals and its verifiable characteristics, with citations |
| step `note` | Technique tips, each with the reason it works |

## Getting recipes into the web app

Recipes are stored as ordinary `.crisp` files in `CRISP_LIBRARY_DIR`. To get
one into the web app, either:

- *Import* the file (or drag and drop it); or
- ask the model to publish it, then open it from the server.

Because the recipe is signed with your own key, edits you make in the app are
revisions of the same recipe.
