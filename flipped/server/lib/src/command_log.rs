use std::collections::HashMap;

use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::credentials::AccessRole;
use crate::error::{SessionApplicationError, SessionErrorCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation {
    Start,
    Advance,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedCommandResult<T> {
    pub value: std::result::Result<T, SessionApplicationError>,
}

#[derive(Debug)]
pub struct CommandLog<T> {
    capacity: usize,
    bindings: HashMap<(AccessRole, Uuid, Uuid), (Operation, [u8; 32])>,
    results: HashMap<(AccessRole, Uuid, Operation, Uuid), CachedCommandResult<T>>,
}

impl<T: Clone> CommandLog<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            bindings: HashMap::new(),
            results: HashMap::new(),
        }
    }

    pub fn lookup(
        &self,
        role: AccessRole,
        jti: Uuid,
        command_id: Uuid,
        operation: Operation,
        input_hash: [u8; 32],
    ) -> std::result::Result<Option<CachedCommandResult<T>>, SessionErrorCode> {
        if let Some((bound_operation, bound_hash)) = self.bindings.get(&(role, jti, command_id)) {
            if *bound_operation != operation || *bound_hash != input_hash {
                return Err(SessionErrorCode::InvalidCommandId);
            }
        }
        Ok(self
            .results
            .get(&(role, jti, operation, command_id))
            .cloned())
    }

    pub fn has_capacity(&self) -> bool {
        self.results.len() < self.capacity
    }

    pub fn insert(
        &mut self,
        role: AccessRole,
        jti: Uuid,
        command_id: Uuid,
        operation: Operation,
        input_hash: [u8; 32],
        result: CachedCommandResult<T>,
    ) -> std::result::Result<(), SessionErrorCode> {
        if self.results.len() >= self.capacity {
            return Err(SessionErrorCode::CommandCapacityExceeded);
        }
        self.bindings
            .insert((role, jti, command_id), (operation, input_hash));
        self.results
            .insert((role, jti, operation, command_id), result);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.results.len()
    }
}

pub fn command_input_hash(session_id: &str, operation: Operation) -> [u8; 32] {
    let operation = match operation {
        Operation::Start => "start",
        Operation::Advance => "advance",
        Operation::End => "end",
    };
    let mut digest = Sha256::new();
    for value in [session_id, operation] {
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value.as_bytes());
    }
    digest.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_retry_returns_cached_result_and_cross_operation_reuse_is_rejected() {
        let role = AccessRole::Examiner;
        let jti = Uuid::now_v7();
        let command_id = Uuid::now_v7();
        let start_hash = command_input_hash("session", Operation::Start);
        let mut log = CommandLog::new(2);
        log.insert(
            role,
            jti,
            command_id,
            Operation::Start,
            start_hash,
            CachedCommandResult { value: Ok(7_u64) },
        )
        .expect("first command is stored");

        let retry = log
            .lookup(role, jti, command_id, Operation::Start, start_hash)
            .expect("exact binding is valid")
            .expect("exact result is cached");
        assert_eq!(retry.value, Ok(7));
        assert_eq!(
            log.lookup(
                role,
                jti,
                command_id,
                Operation::End,
                command_input_hash("session", Operation::End),
            ),
            Err(SessionErrorCode::InvalidCommandId)
        );
    }

    #[test]
    fn rejected_result_remains_deterministic_and_binds_operation() {
        let role = AccessRole::Examiner;
        let jti = Uuid::now_v7();
        let command_id = Uuid::now_v7();
        let hash = command_input_hash("session", Operation::Advance);
        let rejected = SessionApplicationError::new(SessionErrorCode::InvalidState, 4);
        let mut log = CommandLog::<u64>::new(2);
        log.insert(
            role,
            jti,
            command_id,
            Operation::Advance,
            hash,
            CachedCommandResult {
                value: Err(rejected),
            },
        )
        .expect("rejection is authoritative");

        assert_eq!(
            log.lookup(role, jti, command_id, Operation::Advance, hash)
                .expect("same operation remains bound")
                .expect("rejection is cached")
                .value,
            Err(rejected),
        );
        assert_eq!(
            log.lookup(
                role,
                jti,
                command_id,
                Operation::Start,
                command_input_hash("session", Operation::Start),
            ),
            Err(SessionErrorCode::InvalidCommandId),
        );
    }

    #[test]
    fn capacity_is_never_evicted() {
        let mut log = CommandLog::new(1);
        let jti = Uuid::now_v7();
        let command_id = Uuid::now_v7();
        let hash = command_input_hash("session", Operation::Start);
        log.insert(
            AccessRole::Examiner,
            jti,
            command_id,
            Operation::Start,
            hash,
            CachedCommandResult { value: Ok(1_u64) },
        )
        .expect("first result fits");
        assert!(!log.has_capacity());
        assert_eq!(log.len(), 1);
    }
}
