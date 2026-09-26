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

/// The thing a request touches, with its current ownership fact.
///
/// Ownership is loaded fresh per request from the database, so an out-of-band
/// reassignment takes effect on the next request (spec FR-16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resource {
    /// A character sheet; its owning account is its sole writer.
    Character { owner_sub: String },
    /// An effect; its creating account is its sole writer, regardless of
    /// whose characters it targets.
    Effect { creator_sub: String },
    /// A `custom`-lane content row; its creating account is its sole writer.
    CustomRow { creator_sub: String },
}

impl Resource {
    /// The `sub` that alone may write this resource.
    #[must_use]
    pub fn sole_writer(&self) -> &str {
        match self {
            Resource::Character { owner_sub } => owner_sub,
            Resource::Effect { creator_sub } | Resource::CustomRow { creator_sub } => creator_sub,
        }
    }
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
/// - `Write` → allow only for the resource's sole writer (FR-8/9/11).
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
            } else if actor.sub == resource.sole_writer() {
                Verdict::Allow
            } else {
                Verdict::Deny
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/authz.rs"]
mod tests;
