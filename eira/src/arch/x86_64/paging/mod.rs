//! x86_64 paging implementation.

pub mod entry;
pub mod mapper;
pub mod table;

#[allow(unused_imports)]
pub use entry::{EntryFlags, PageTableEntry};
