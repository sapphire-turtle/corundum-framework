//! Paging through the history.

mod common;

use common::*;
use corundum_framework_journal::{BatchId, JournalConfig, JournalError, MemoryStore};

fn submit_and_wait(j: &corundum_framework_journal::Journal<Shared>) -> BatchId {
    let id = j.submit(vec![set("a", 1)], &gui(), &u1(), None).unwrap();
    j.wait(id).unwrap();
    id
}

#[test]
fn a_cursor_the_store_dropped_is_not_found() {
    let (j, _) = journal_with_store(MemoryStore::new(2), JournalConfig::default());
    let first = submit_and_wait(&j);
    submit_and_wait(&j);
    submit_and_wait(&j);
    assert!(matches!(
        j.history(Some(first), 10),
        Err(JournalError::NotFound(id)) if id == first
    ));
}

#[test]
fn the_oldest_kept_cursor_is_the_end() {
    let (j, _) = journal_with_store(MemoryStore::new(2), JournalConfig::default());
    submit_and_wait(&j);
    let page = j.history(None, 10).unwrap();
    let oldest = page.last().unwrap().begin.batch_id;
    assert_eq!(j.history(Some(oldest), 10).unwrap(), vec![]);
}
