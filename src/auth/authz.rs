//! The ownership matrix, whole, as a pure function.
//!
//! Players own their sheets, casters own their effects, the GM watches
//! (Constitution Article VI). This module IS that promise, enumerated: no I/O,
//! no framework imports, testable as a table. Shell layers (middleware,
//! extractors) call [`authorize`]; they never re-derive verdicts inline.

/// What an account may do at the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// A player account: owns a character, casts effects.
    Player,
    /// The GM account: reads everything, writes nothing.
    Gm,
}

impl Role {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Player => "player",
            Role::Gm => "gm",
        }
    }

    /// Parse the `role` column value; unknown values read as `player` is NOT
    /// acceptable, so this returns an option — callers decide what to do.
    #[must_use]
    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "player" => Some(Role::Player),
            "gm" => Some(Role::Gm),
            _ => None,
        }
    }
}

/// Who is acting. Built from the server-held session, never from request data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub sub: String,
    pub role: Role,
}

/// The thing a request touches, with its current authorization facts.
///
/// Facts are loaded fresh per request from the database at the call site; the
/// matrix performs no lookups (E3's ownership-loaded-fresh rule, spec FR-16).
/// Party-scoped resources (E12 design D4) carry the deciding facts in the
/// payload — a lookup key is not a deciding fact, so a bare party id would
/// decide nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resource {
    /// A character sheet; its owning account is its sole writer.
    Character { owner_sub: String },
    /// An effect; its creating account is its sole writer, regardless of
    /// whose characters it targets.
    Effect { creator_sub: String },
    /// A `custom`-lane content row; its creating account is its sole writer.
    CustomRow { creator_sub: String },
    /// A party stash write — add, qty, transfer, meta (E12 design D4; spec
    /// FR-1). The shared loot list is the party's, not one owner's: any
    /// account owning a character in the party writes it. The call site
    /// loads membership fresh per message and passes the fact. Denials audit
    /// as `forbidden_stash_write`.
    Stash { actor_is_party_member: bool },
    /// A party bank write — sell, manual adjustment (E12 design D4; spec
    /// FR-7). Money moves are quartermaster-only: the payload is the `sub`
    /// of the account owning the character named by
    /// `parties.quartermaster_character_id`, and only that account passes.
    /// `None` — the undesignated party — denies everyone (FR-5's
    /// undesignated case). Denials audit as `forbidden_bank_write`.
    Bank {
        quartermaster_owner_sub: Option<String>,
    },
    /// A party settings write — the quartermaster designation field (E12
    /// design D4). Per FR-5's settled flip rule (Clarify Log #6, the
    /// owner-visible reading) any character owner may flip the designation;
    /// the designated owner holds no exclusive lock on the toggle. Denials
    /// audit as `forbidden_party_write`.
    PartySettings { actor_is_party_member: bool },
}

/// What is being attempted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Read,
    Write,
}

/// The outcome of an authorization decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Deny,
}

/// Decide one (actor, action, resource) triple. This function is the matrix:
///
/// - `Read` → allow for any authenticated, allowlisted actor. Ownership gates
///   writes; nothing gates reads (FR-13). Unauthenticated actors never reach
///   this function — they are rejected upstream as unauthenticated.
/// - `Write` by the GM → deny, independent of resource (FR-12).
/// - `Write` → per resource: sole-owner resources (`Character`, `Effect`,
///   `CustomRow`) allow only their sole writer (FR-8/9/11); the party-scoped
///   resources read the facts carried in their payload (E12 design D4) —
///   stash and party settings allow any party member, bank allows only the
///   designated quartermaster's owner and denies undesignated outright.
///
/// Effect creation is not a [`Resource`] case: the creation endpoint checks
/// the actor owns a character (FR-10) before constructing an effect.
#[must_use]
pub fn authorize(actor: &Actor, action: Action, resource: &Resource) -> Verdict {
    match action {
        Action::Read => Verdict::Allow,
        Action::Write => {
            if actor.role == Role::Gm {
                Verdict::Deny
            } else {
                match resource {
                    Resource::Character { owner_sub }
                    | Resource::Effect {
                        creator_sub: owner_sub,
                    }
                    | Resource::CustomRow {
                        creator_sub: owner_sub,
                    } => {
                        if *owner_sub == actor.sub {
                            Verdict::Allow
                        } else {
                            Verdict::Deny
                        }
                    }
                    Resource::Stash {
                        actor_is_party_member,
                    }
                    | Resource::PartySettings {
                        actor_is_party_member,
                    } => {
                        if *actor_is_party_member {
                            Verdict::Allow
                        } else {
                            Verdict::Deny
                        }
                    }
                    Resource::Bank {
                        quartermaster_owner_sub,
                    } => {
                        if quartermaster_owner_sub.as_deref() == Some(actor.sub.as_str()) {
                            Verdict::Allow
                        } else {
                            Verdict::Deny
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/authz.rs"]
mod tests;
