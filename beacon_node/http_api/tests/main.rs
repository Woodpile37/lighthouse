#![cfg(not(debug_assertions))] // Tests are too slow in debug.
#![recursion_limit = "512"]

pub mod fork_tests;
pub mod interactive_tests;
pub mod status_tests;
pub mod tests;
