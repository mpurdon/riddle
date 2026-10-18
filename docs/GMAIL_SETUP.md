# Connecting Gmail

Riddle reaches Gmail through the Gmail API, which only accepts OAuth 2.0
access tokens, and Google only issues those to a registered OAuth client. So
a client id is unavoidable — there is no password or app-password door into
the API any more, and the IMAP one would cost labels, threads, the history
cursor and `batchModify`, which Sift depends on (`ARCHITECTURE.md` §16).

The client id is not a credential and not a secret. It identifies *the
application* to Google, separately from you, which is what makes the consent
screen, the revocation list in your account, per-app quota and Workspace
admin controls possible. Riddle registers as a **Desktop app**, a public
client that uses PKCE and a loopback redirect (RFC 8252), so there is no
client secret to protect and none to paste.

Console facts here were checked on 2026-09-22 against Google's own guides;
policy details in the last section were checked on 2026-09-21 and are cited
in `ARCHITECTURE-REVIEW.md` §5.2. Google reorganised this console in 2025 —
what used to be one "OAuth consent screen" page is now **Google Auth
Platform** with Branding, Audience, Data Access and Clients tabs — so if a
label has moved again, the shape of the task is still the one below.

## A client baked into the build

A build of Riddle can carry its own Desktop client, so a new person signs
in without opening the Google Cloud console at all. The repository ships
none: the id and secret are read at compile time from the environment, and
a build without them behaves exactly as this document describes.

    RIDDLE_GOOGLE_CLIENT_ID=1234567890-abc.apps.googleusercontent.com \
    RIDDLE_GOOGLE_CLIENT_SECRET=GOCSPX-... \
    pnpm tauri build

(`cargo build -p riddle-cli` takes the same variables.) The values land in
`riddle-settings` through `option_env!` (`GmailSettings::builtin_client`);
its `build.rs` tells Cargo to rebuild when either variable changes, so a
stale cache cannot ship a build that says it has a client and has not.

What changes with a client baked in:

- The Add account wizard leaves the Google client step out; Settings →
  Accounts → Google sign-in says "Using the built-in Google client".
- **Bring your own remains.** "Use your own" on that panel sets
  `gmail.useBuiltinClient = false` in `settings.toml` and opens the steps
  below; a client you import or type (`gmail.clientId`) is used whenever
  its id is set, built-in or not. A Workspace domain that requires an
  Internal client needs your own.
- The secret is a Desktop client's: Google treats the client as public (it
  signs in with PKCE and a loopback redirect, RFC 8252), so the secret does
  not protect anything and may be compiled in. Riddle still never prints or
  logs it; `GoogleClient`'s `Debug` says only whether there is one, and the
  diagnostics export carries the client as present-or-absent.
- `settings.gmail.builtinClientAvailable` is derived at read time from the
  build and never written to the file.

## First decide: Internal or External

`gmail.modify` is a **restricted** scope, and that choice decides how much
process stands between you and a working mailbox.

| | Verification | Refresh token life | Who can connect |
|---|---|---|---|
| **Internal** (a Workspace domain you administer) | none | normal | only accounts on that domain |
| **External**, Testing | none, up to 100 test users | **expires 7 days after consent** | any Google account |
| **External**, published and verified | OAuth review **plus** an annual CASA security assessment | normal | anyone |

For a work mailbox on a Workspace domain, choose **Internal**: no
verification, no assessment, no weekly re-authorisation. A personal
`@gmail.com` account cannot use an Internal client, so it needs a separate
**External** one, where Testing status means re-consenting every seven days.
Publishing an External app is meant to lift that expiry even before
verification completes, at the cost of an "unverified app" warning screen and
a cap on new grants — worth trying before committing to a CASA assessment,
but treat it as something to confirm rather than a promise.

## In the Google Cloud console

1. **Create or pick a project.** <https://console.cloud.google.com/> → the
   project picker → **New project**. Name it whatever you like ("Riddle").
   No billing account is needed; the Gmail API is free within quota.

2. **Enable the Gmail API.** Menu → **APIs & Services** → **Library** →
   search "Gmail API" → **Enable**. Nothing works until this is on, and the
   error when it is off is an opaque 403.

3. **Configure the auth platform.** Menu → **Google Auth platform**.
   - **Branding**: set **App name** (this is the text on the consent screen —
     "Riddle" is what you will be agreeing to grant access to) and **User
     support email**.
   - **Audience**: choose the user type. **Internal** if this is a Workspace
     domain you administer and you only connect accounts on it; otherwise
     **External**. If you chose External, add yourself under **Test users**
     → **Add users** — an External app in Testing will refuse anyone not on
     that list.
   - **Data Access** → **Add or remove scopes**: add exactly one scope,

     ```
     https://www.googleapis.com/auth/gmail.modify
     ```

     Riddle asks for nothing else. `gmail.modify` covers reading, labelling,
     archiving, drafting and sending; it deliberately does not include
     `https://mail.google.com/`, which would add permanent deletion.

4. **Create the client.** Menu → **Google Auth platform** → **Clients** →
   **Create client** → **Application type: Desktop app** → give it a name
   (console-only, nobody else sees it) → **Create**.

   A Desktop app client needs **no authorised redirect URIs**: Google accepts
   loopback redirects on any port, which is what lets Riddle bind
   `http://127.0.0.1:<random port>/callback` fresh for each sign-in.

5. **Download the client JSON.** The dialog after **Create** (or the
   client's row under **Clients**) offers **Download JSON**, a
   `client_secret_<id>.json` file holding the client id and, if Google
   issued one, the client secret. Keep it out of this repository and delete
   it once imported. Or copy the client id, which looks like
   `1234567890-abcdefghijklmnopqrstuvwx.apps.googleusercontent.com`, and the
   secret by hand.

## In Riddle

1. **Settings → Accounts → Add account → Gmail.** The wizard walks the rest:

   - **Google client.** Import the downloaded JSON (or enter the id and
     secret by hand). This happens once; later accounts skip the step. The
     client id goes to `settings.toml` in the clear, which is correct: it is
     not a secret. The client secret goes to the keychain, and a file
     without one removes any stored secret. A file for a *Web application*
     client is refused; create a Desktop app client instead. The same
     controls live under **Settings → Accounts → Google sign-in**.
   - **Behaviour.** Name, group, autonomy, Gmail changes and privacy. New
     accounts start at **Observe** with **no Gmail changes**, so you can
     look at what Riddle makes of the mail before it touches anything.
   - **Sign in.** In a development build the wizard first asks you to arm
     it (hold the button): a dev build refuses to talk to a real mailbox
     until armed. Release builds skip this. Then your browser opens
     Google's consent screen; approve it and come back. The wizard waits,
     shows the account when Google returns, and says why if it does not.

2. **Afterwards**, everything about the account is in its pane under
   **Settings → Accounts**: status, Sync now, Test connection, autonomy,
   Gmail changes, privacy, group, and Remove. The "Right now" line there
   says in one sentence what Riddle will do to the mailbox.

The same thing from the command line, which is useful when the window is not
running:

```bash
riddle serve &                                    # the bridge prints its port and token
scripts/bridge.py invoke connect_gmail '{"name":"Work"}' --yes
# → {"digId": "...", "authUrl": "https://accounts.google.com/o/oauth2/v2/auth?..."}
open "<authUrl>"
```

The Action returns as soon as the consent URL exists, so nothing hangs while
you are in the browser; the sign-in finishes on its own and the Dig appears
when Google comes back.

## What to expect on the first sync

Gmail allows about 6,000 quota units per user per minute and a message fetch
costs 20, so roughly **300 messages a minute**: a 20,000-message mailbox
takes over an hour, and 100,000 takes most of a day. That is a provider
limit, not a Riddle one. The Task Centre shows the progress, the summary says
"Catching up" rather than pretending to be done, and Sift works through the
backlog newest first so the recent mail is understood long before the
archive is.

Riddle polls `history.list` every 30–60 seconds after that, at 2 units a
call. Gmail's history cursor is usually valid for about a week but can expire
in hours; when it does, Riddle performs a full resync and reconciles labels
against what it expected, reporting the differences in Activity instead of
overwriting anything.

## When it goes wrong

| What you see | What it means |
|---|---|
| `admin_policy_enforced` | A Workspace administrator blocks unconfigured third-party apps. They need to mark your client id Trusted in the admin console; Riddle shows this error with what to ask for. |
| "Google did not return a refresh token" | Google only issues one on first consent. Remove Riddle from the account's third-party access list (<https://myaccount.google.com/connections>), then open the account in Settings → Accounts and choose **Sign in again**. |
| Sign-in works, then stops after a week | The app is External and still in Testing: authorisations expire seven days after consent. **Sign in again** from the account in Settings → Accounts keeps everything Riddle learned; publish the app, or move to an Internal client on a Workspace domain, to stop it recurring. |
| An "unverified app" warning | Expected for an External app that has not been through verification. Fine for your own use; not something to put in front of other people. |
| Sign-in fails after many reconnects | Google allows 100 refresh tokens per client id per account; the oldest are revoked silently. |

## Policy, briefly

`gmail.modify` being restricted brings two obligations that matter only if you
take Riddle beyond your own use:

- **A CASA security assessment** (annual, third-party, typically quoted in the
  hundreds to low thousands of dollars) applies when an app transmits
  restricted-scope data off the device. Riddle does that whenever a Dig sends
  message text to the judge or the escalation model — so the per-Dig
  **local-only** privacy mode is also the lever that avoids the trigger, not
  just a user preference.
- **Google's Limited Use policy** allows that transfer only for a prominent,
  consented, user-facing feature, with a vendor that does not train on the
  data, and now requires prompt-injection protection. Riddle states what
  leaves the device in Settings → Privacy, and the threat model in
  `ARCHITECTURE.md` §58 is the injection half of that requirement.

## Where the pieces live

- **Client id**: `settings.toml` under the app's config directory — not a
  secret, safe in version control if you ever want it there.
- **Refresh token**: the macOS keychain, service `dev.codenaked.riddle`
  (dev builds use `dev.codenaked.riddle.dev`, a separate service, so a
  development build can never reach a mailbox a release build connected).
  The webview is only ever told whether a credential is present, never its
  value.
- **Signing in again**: open the account in Settings → Accounts and choose Sign in again. The refresh token is replaced under the same keychain key; the account keeps its name, settings, corrections and history, and a catch-up runs.
- **Disconnecting**: remove the Dig in Settings. That purges its store, its
  traces, its exports and its keychain entry. To revoke from Google's side as
  well, remove Riddle at <https://myaccount.google.com/connections>.
