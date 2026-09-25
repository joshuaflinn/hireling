//! Tests for [`super`] — the ownership matrix as data.
//!
//! The whole matrix is asserted here as rows: every role × every resource
//! kind × every action. A verdict that drifts from the spec fails this table,
//! not a player's session.

use super::Action;
use super::Actor;
use super::Resource;
use super::Role;
use super::Verdict;
use super::authorize;

fn actor(sub: &str, role: Role) -> Actor {
    Actor {
        sub: sub.to_owned(),
        role,
    }
}

fn resources() -> Vec<Resource> {
    vec![
        Resource::Character {
            owner_sub: "owner".to_owned(),
        },
        Resource::Effect {
            creator_sub: "caster".to_owned(),
        },
        Resource::CustomRow {
            creator_sub: "author".to_owned(),
        },
    ]
}

#[test]
fn reads_are_allowed_for_every_authenticated_role_on_every_resource() {
    // Unauthenticated actors never reach authorize() — the auth layer rejects
    // them first. For everyone who does reach it, reads are unrestricted.
    for resource in resources() {
        for actor in [
            actor("someone-else", Role::Player),
            actor("owner", Role::Player),
            actor("caster", Role::Player),
            actor("the-gm", Role::Gm),
        ] {
            assert_eq!(
                authorize(&actor, Action::Read, &resource),
                Verdict::Allow,
                "{actor:?} reading {resource:?} must be allowed — ownership gates \
                 writes, never reads"
            );
        }
    }
}

#[test]
fn the_owner_is_the_sole_writer_of_their_character() {
    let character = Resource::Character {
        owner_sub: "owner".to_owned(),
    };
    assert_eq!(
        authorize(&actor("owner", Role::Player), Action::Write, &character),
        Verdict::Allow,
        "the owning account writes its own character"
    );
    for other in ["not-the-owner", "caster", "author", "the-gm"] {
        let role = if other == "the-gm" {
            Role::Gm
        } else {
            Role::Player
        };
        assert_eq!(
            authorize(&actor(other, role), Action::Write, &character),
            Verdict::Deny,
            "{other} writing someone's character must be denied"
        );
    }
}

#[test]
fn the_caster_is_the_sole_writer_of_their_effect_even_on_other_sheets() {
    // Bear's Bless lives on Josh's and Becky's sheets; Bear still writes it.
    let effect = Resource::Effect {
        creator_sub: "caster".to_owned(),
    };
    assert_eq!(
        authorize(&actor("caster", Role::Player), Action::Write, &effect),
        Verdict::Allow,
        "the effect's creator writes it regardless of whose sheet it sits on"
    );
    for other in ["owner", "not-the-owner", "the-gm"] {
        let role = if other == "the-gm" {
            Role::Gm
        } else {
            Role::Player
        };
        assert_eq!(
            authorize(&actor(other, role), Action::Write, &effect),
            Verdict::Deny,
            "{other} writing an effect they did not create must be denied — \
             being a target's owner grants nothing"
        );
    }
}

#[test]
fn a_custom_rows_creator_is_its_sole_writer() {
    let row = Resource::CustomRow {
        creator_sub: "author".to_owned(),
    };
    assert_eq!(
        authorize(&actor("author", Role::Player), Action::Write, &row),
        Verdict::Allow
    );
    assert_eq!(
        authorize(&actor("someone-else", Role::Player), Action::Write, &row),
        Verdict::Deny
    );
    assert_eq!(
        authorize(&actor("the-gm", Role::Gm), Action::Write, &row),
        Verdict::Deny
    );
}

#[test]
fn the_gm_writes_nothing_of_any_kind() {
    let gm = actor("the-gm", Role::Gm);
    for resource in resources() {
        assert_eq!(
            authorize(&gm, Action::Write, &resource),
            Verdict::Deny,
            "GM writing {resource:?} must be denied — even if the GM somehow \
             appeared as the sole writer, role denies first"
        );
    }
    // Even a character recorded as GM-owned stays unwritable by the GM.
    let gm_owned = Resource::Character {
        owner_sub: "the-gm".to_owned(),
    };
    assert_eq!(
        authorize(&gm, Action::Write, &gm_owned),
        Verdict::Deny,
        "the GM role denies every write, including on its own row"
    );
}

#[test]
fn ownership_is_evaluated_per_request_against_current_facts() {
    // The verdict uses whatever ownership fact the caller loaded — so an
    // out-of-band reassignment (new owner_sub loaded from the DB) flips the
    // verdict on the next request with no session action.
    let before = Resource::Character {
        owner_sub: "old-owner".to_owned(),
    };
    let after = Resource::Character {
        owner_sub: "new-owner".to_owned(),
    };
    assert_eq!(
        authorize(&actor("old-owner", Role::Player), Action::Write, &before),
        Verdict::Allow
    );
    assert_eq!(
        authorize(&actor("old-owner", Role::Player), Action::Write, &after),
        Verdict::Deny,
        "after reassignment the former owner is a non-owner"
    );
    assert_eq!(
        authorize(&actor("new-owner", Role::Player), Action::Write, &after),
        Verdict::Allow
    );
}

#[test]
fn identity_is_matched_exactly_not_by_prefix_or_case() {
    let character = Resource::Character {
        owner_sub: "owner".to_owned(),
    };
    assert_eq!(
        authorize(
            &actor("owner-with-a-tail", Role::Player),
            Action::Write,
            &character
        ),
        Verdict::Deny
    );
    assert_eq!(
        authorize(&actor("OWNER", Role::Player), Action::Write, &character),
        Verdict::Deny
    );
}
