# Roadmap

Work that is agreed in direction but not scheduled. `BUILD_PLAN.md` holds
the known gaps of the first build and what waits on the user; this file
holds what comes next. Each entry says why it matters and what is already
there to build on.

## Rules that travel between machines

**Why.** v1 is single-device (ARCHITECTURE.md, "Multi-device"). Meshes,
Signals, the Never Hidden list and the rest of the profile live in one
SQLite file (`index.sqlite` in the app's data directory). A second Mac, a
reinstall or a deleted data folder starts again from the default Meshes;
mail comes back from Gmail, the rules that decide it do not.

**What exists.** `export_meshes` / `import_meshes` Actions write and read
Mesh DSL text, importing as Drafts so nothing acts without the user. No
screen calls them yet. Signals and the profile have no export.

**Steps, in order:**

1. **Export and import in Settings.** One file with Meshes, Signals and the
   Never Hidden list (and VIPs, family, organisation domains). Import lands
   as Drafts and pending profile changes the user accepts; it never
   overwrites a rule silently. Needs: a Signal export/import and a profile
   export, alongside the Mesh ones that exist.
2. **Automatic backup.** Write that file to a folder the user picks (iCloud
   Drive, Dropbox) whenever a rule changes, so a new machine imports the
   latest without anyone remembering to export.
3. **Sync through a Git repository (GitHub or any remote).** Riddle keeps
   its policy as a directory of plain text, one file per Mesh and Signal
   plus the profile lists, commits on every change with the change note as
   the message, and pulls on start and on demand. Beyond moving rules
   between machines this gives a readable history and diff of every policy
   change, which Activity alone does not. Pulls import as Drafts or as a
   new version beside the current one; two machines that changed the same
   Mesh produce two versions for the user to choose between, never a merge
   Riddle guesses. Credentials come from the system (`gh auth`, the
   keychain), never from the repository.

   **Privacy.** The repository must be private, and Riddle should refuse a
   public one. The files carry personal data even though they carry no mail:
   Mesh rules name senders (`sender = "reminders@jotsandnibbles.com"`), the
   Never Hidden, VIP and family lists are the addresses of the people who
   matter most to the user, and Signal definitions can name people and
   projects. A private repository still sits with a third party and every
   integration granted access to it, so two further options belong in the
   design:
   - encrypt the address-bearing files (the profile lists, and rules that
     name an address) with a key held in the keychain, leaving the rule
     structure readable for diffs;
   - or keep addresses out of the synced files, referring to named lists
     (`sender IN never_hidden`) whose contents sync only through the
     encrypted path.

   Secrets (tokens, API keys) are never written to it, as everywhere else
   in this repository.

4. **Not planned: live multi-device sync of decisions.** Two machines
   deciding the same mailbox under different rules would disagree; Gmail
   is the shared state, and the rules are what travels.

## Onboarding with packs

**Why.** The pack machinery exists (named lists, packs, bands, install and
share, the rule check), but a new person still meets the Packs page cold.

**Next.** A scan of the first 90 days of headers and facts (no judge, no
cost) that reads each pack's `detect` block and proposes the packs that
apply with counts ("GitHub would handle 2,324 of your messages"), pre-fills
each list's answers from the mail (frequent promotion senders for
`keep_promotions`, repositories from `list_id`), simulates, installs in
Shadow, and opens the rule check. Pack updates shown as a diff with a
simulation before they replace a pack's Meshes. Packs that carry their own
Signals (today a pack may only use the system vocabulary).

## Deferred from the Replaces, cadence and fall-through work (Sept 2026)

Decided against for now, recorded so they are not rediscovered:

- **Fall-through never hides, generally.** A borderline Find may not fall
  through to Spoil or Contamination; a borderline Filed still may. The
  general rule orders dispositions by visibility (Find, Filed, Spoil,
  Contamination) and lets a later Mesh take over only at the same or a more
  visible level. It changes behaviour and wants the user's call.
- **Never Hidden resolved in the backend.** The app mirrors the matching
  rule in TypeScript to find which entry covers a sender; removing a domain
  entry from one sender's button removes it for everyone in the domain
  without a confirmation. The backend should resolve the entry and confirm
  domain removals.
- **User-extensible government domains.** `government_sender` knows
  Canadian federal, provincial and territorial domains, `.gov`/`.mil` and
  `gov.xx`-style domains. Municipal and school-board senders
  (`toronto.ca`, `ottawa.ca`) need a user list merged into the built-ins.
- **One series mechanism.** Finds and messages each settle `+ Replaces`
  series with the same rule (newest message wins, kept members stay out);
  Finds span Digs in the index, messages live per Dig. Unifying them is a
  redesign.
- **The Director stops a loop of successful calls.** It stops when the same
  call fails twice; the same call succeeding over and over is caught only
  by the round limit.
- **One grammar implementation.** The browser mock re-implements the Mesh
  parser, held to the Rust one by a shared corpus. A WebAssembly build of
  `riddle_mesh::parse` would remove the copy.
