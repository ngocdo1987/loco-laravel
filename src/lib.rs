pub mod app;
pub mod controllers;
pub mod data;
pub mod dtos;
pub mod initializers;
// `tasks` and `mailers` are intentionally-empty generator landing zones —
// see the doc comment in each `mod.rs`. Feature code lives in `modules/`.
pub mod mailers;
pub mod models;
pub mod modules;
pub mod tasks;
pub mod workers;
