# Riddle UX study

A static site: the usability audit of the desktop client, what mail and
communication clients teach, the research on attention and trust in
automation, seven expert dossiers, and the proposals with a pick control per
flow. Plain HTML and CSS, no build step; edit the files and reload.

| Page | What it holds |
|---|---|
| `index.html` | The evidence: verdict, the top twenty problems, what to keep, vocabulary, clients, research, experts, principles |
| `proposals.html` | Three directions, twelve flows with options, wireframes, evidence and a recommendation each; a pick control per flow |
| `experts/*.html` | One dossier per expert: ideas, contributions, verified quotes, implications for Riddle, sources |
| `research/` | The source files the pages were written from, with a viewer at `research/index.html` |
| `assets/` | The shared stylesheet and the wireframe kit |

## Serve it

Everything is relative, so any static server pointed at this folder works.
From the repository root:

```sh
scripts/explore.sh          # http://127.0.0.1:8181
scripts/explore.sh 9000     # another port
```

The script uses the first of these it finds, so nothing has to be installed:

| Server | Install | One-liner |
|---|---|---|
| static-web-server (Rust, single binary, the "tiny nginx") | `brew install static-web-server` | `static-web-server -d explore -p 8181 --host 127.0.0.1` |
| Caddy | `brew install caddy` | `caddy file-server --root explore --listen 127.0.0.1:8181` |
| miniserve (Rust) | `brew install miniserve` | `miniserve explore --index index.html -i 127.0.0.1 -p 8181` |
| Python, already present | none | `python3 -m http.server 8181 --bind 127.0.0.1 --directory explore` |

The pages load two things from the network: Google Fonts (they fall back to
system faces offline) and, on the research viewer only, a Markdown renderer
from cdnjs (offline it shows the raw text). Nothing else leaves the machine.
Opening the HTML files directly from disk also works, except the research
viewer, which needs HTTP to fetch the Markdown.

## Recording picks

Picks on the proposals page are kept in the browser's local storage. When
done, use "Copy picks as Markdown" in the bar at the top of the page and
paste the result into the chat, or "Download JSON". "Clear" needs a second
click.

## Provenance

Written 25 September 2026 from a source audit of `apps/desktop/src`, fetched
primary sources for every client, study and expert (URLs are in each page's
Sources section and in `research/`), and the house conventions in
`APPLICATION_FEATURES.md`. Quotations are fifteen words or fewer and were
verified against the cited page; anything that could not be verified is
marked as such in the page's "What we could not confirm" note.
