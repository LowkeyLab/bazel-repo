use std::{cell::RefCell, io, rc::Rc};

use googletest::{assert_that, matchers::eq};

use crate::diagnostics::{CommandObserved, Diagnostics, Listener};
struct Failing;
impl Listener for Failing {
    fn observe(&mut self, _: &CommandObserved) -> io::Result<()> {
        Err(io::Error::other("secret diagnostic destination details"))
    }
}
struct Capture(Rc<RefCell<Vec<CommandObserved>>>);
impl Listener for Capture {
    fn observe(&mut self, fact: &CommandObserved) -> io::Result<()> {
        self.0.borrow_mut().push(fact.clone());
        Ok(())
    }
}
#[googletest::test]
fn failed_listener_preserves_the_committed_fact_and_independent_delivery() {
    let captured = Rc::new(RefCell::new(Vec::new()));
    let fact = CommandObserved {
        command: "catalog init".into(),
        outcome: "completed".into(),
        reason_code: "ok".into(),
        catalog_id: Some("a265d880-b416-4a50-905b-3ee9c64f71dd".into()),
        revision: Some(1),
    };
    let mut listeners = Diagnostics::new(vec![
        ("broken", Box::new(Failing)),
        ("capture", Box::new(Capture(captured.clone()))),
    ]);
    let warnings = listeners.observe(&fact);
    assert_that!(
        warnings.as_slice(),
        eq(["diagnostic_listener_failed"].as_slice())
    );
    assert_that!(
        captured.borrow().as_slice(),
        eq(std::slice::from_ref(&fact))
    );
}
