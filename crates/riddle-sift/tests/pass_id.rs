//! Every input carries the id of its pass (`SiftInput::pass_id`): a pass
//! stamps one id on each input it builds, so a window's decisions can be
//! read together, and an input built on its own is a pass of one.

use riddle_core::*;
use riddle_sift::{new_pass_id, DigContext, PeopleFacts, SiftInput};

fn input(provider_id: &str) -> SiftInput {
    let message = riddle_mail::fixture::blank_message("d1", provider_id);
    SiftInput::simple(
        message,
        UserProfile::default(),
        PeopleFacts::first_time(),
        DigContext::fixture("d1"),
    )
}

#[test]
fn an_input_built_on_its_own_is_a_pass_of_one() {
    let a = input("p1");
    let b = input("p2");
    assert!(uuid::Uuid::parse_str(&a.pass_id).is_ok(), "{}", a.pass_id);
    assert_ne!(
        a.pass_id, b.pass_id,
        "two inputs built apart are two passes"
    );
}

#[test]
fn a_pass_stamps_the_same_id_on_every_input_it_builds() {
    let pass_id = new_pass_id();
    let inputs: Vec<SiftInput> = (0..5)
        .map(|i| {
            let mut input = input(&format!("p{i}"));
            input.pass_id = pass_id.clone();
            input
        })
        .collect();
    assert!(inputs.iter().all(|i| i.pass_id == pass_id));
    assert_ne!(new_pass_id(), pass_id, "the next pass is another");
}
