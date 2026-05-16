//! x86_64 paging implementation.

pub mod entry;
pub mod mapper;
pub mod table;

pub use entry::{EntryFlags, PageTableEntry};
