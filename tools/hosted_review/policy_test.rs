//! The timeout contract uses explicit input time.
use googletest::{assert_that, matchers::eq};
use policy::{Decision, Review, decide};
#[googletest::test]
fn timeout_requires_more_than_one_hour() {
    assert_that!(
        decide(true, Review::Incomplete, Some((100, true)), 3699),
        eq(Decision::Waiting)
    );
    assert_that!(
        decide(true, Review::Incomplete, Some((100, true)), 3700),
        eq(Decision::Waiting)
    );
    assert_that!(
        decide(true, Review::Incomplete, Some((100, true)), 3701),
        eq(Decision::Request)
    );
}

#[googletest::test]
fn stopped_and_unconfirmed_reviews_never_establish_timeout() {
    assert_that!(
        decide(false, Review::Incomplete, Some((100, true)), 4000),
        eq(Decision::Closed)
    );
    assert_that!(
        decide(true, Review::Complete, Some((100, true)), 4000),
        eq(Decision::Complete)
    );
    assert_that!(
        decide(true, Review::Missing, Some((100, true)), 4000),
        eq(Decision::Waiting)
    );
    assert_that!(
        decide(true, Review::Incomplete, Some((100, false)), 4000),
        eq(Decision::Uncertain)
    );
    assert_that!(
        decide(true, Review::Incomplete, None, 4000),
        eq(Decision::Untracked)
    );
    assert_that!(
        decide(true, Review::Incomplete, Some((100, true)), 99),
        eq(Decision::Waiting)
    );
}
