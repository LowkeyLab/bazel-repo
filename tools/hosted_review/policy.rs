//! Decide whether a known hosted review needs another request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Review {
    Missing,
    Incomplete,
    Complete,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Closed,
    Complete,
    Untracked,
    Uncertain,
    Waiting,
    Request,
}
/// Only an authenticated, incomplete review can establish timeout eligibility.
pub fn decide(open: bool, review: Review, last: Option<(u64, bool)>, now: u64) -> Decision {
    if !open {
        return Decision::Closed;
    }
    if review == Review::Complete {
        return Decision::Complete;
    }
    let Some((at, confirmed)) = last else {
        return Decision::Untracked;
    };
    if !confirmed {
        return Decision::Uncertain;
    }
    if review == Review::Incomplete && now.checked_sub(at).is_some_and(|elapsed| elapsed > 3600) {
        Decision::Request
    } else {
        Decision::Waiting
    }
}
