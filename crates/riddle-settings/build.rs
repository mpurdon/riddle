//! The built-in Google client is read with `option_env!` at compile time
//! (`GmailSettings::builtin_client`). Cargo does not know an `option_env!`
//! depends on the environment, so without these lines a build after
//! setting `RIDDLE_GOOGLE_CLIENT_ID` reused the cached crate and shipped
//! no client.

fn main() {
    println!("cargo:rerun-if-env-changed=RIDDLE_GOOGLE_CLIENT_ID");
    println!("cargo:rerun-if-env-changed=RIDDLE_GOOGLE_CLIENT_SECRET");
}
