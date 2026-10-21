//! Tool dispatch, split by domain. The router lives in lib.rs; each child
//! module holds the arm bodies for its group, moved verbatim.

mod edits;
mod exec;
mod files;
mod memory;
mod meta;
mod semantic;
mod symbols;
mod transactions;
