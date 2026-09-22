# Security

Riddle handles mail, which is untrusted input, and holds tools that can send
mail and change policy. The threat model and its controls are in
`ARCHITECTURE.md` §58; the incidents that shaped them are in
`ARCHITECTURE-REVIEW.md` §5.3.

## Reporting

Report vulnerabilities privately to the maintainer (see the repository owner
profile). Please include the build stamp shown in Developer Mode → Runtime.
Do not open a public issue for a vulnerability.

## What Riddle does with credentials

- OAuth refresh tokens and API keys live in the macOS keychain (dev builds
  use a separate keychain service). The webview only ever receives
  `*Present` booleans.
- The Jev key can be resolved from AWS SSM Parameter Store at runtime
  (profile and parameter name in settings). It is never written to disk by
  Riddle and never appears in logs, traces, exports or diagnostics.
- Model providers are separate principals from the mailbox.

## What leaves the device

Per Dig: nothing (local-only), normalized message text to the judge
(cloud judgment), or additionally redacted Context to the LLM (cloud
reasoning). Settings → Privacy shows the exact sentence for the current
configuration. Vendors must not train on the data.

## Agent safety

Mail enters model context delimited and marked untrusted. Once a Director
task has read untrusted content, sending, forwarding, creating a Mesh and
editing Never Hidden require explicit approval that shows the Action
layer's real arguments. Durable policy is only ever created by a user
action. Contamination never reaches the Director unless opened deliberately.
