mod csharp;
mod elixir;
mod ffi;
mod go;
mod java;
mod node;
mod php;
mod precommit;
mod python;
mod r;
mod ruby;
mod wasm;

pub(crate) use csharp::scaffold_csharp;
pub(crate) use elixir::{scaffold_elixir, scaffold_elixir_cargo};
pub(crate) use ffi::scaffold_ffi;
pub(crate) use go::scaffold_go;
pub(crate) use java::scaffold_java;
pub(crate) use node::{scaffold_node, scaffold_node_cargo};
pub(crate) use php::{scaffold_php, scaffold_php_cargo};
#[cfg(test)]
pub(crate) use precommit::generate_pre_commit_config;
pub(crate) use precommit::scaffold_pre_commit_config;
pub(crate) use python::{scaffold_python, scaffold_python_cargo};
pub(crate) use r::{scaffold_r, scaffold_r_cargo};
pub(crate) use ruby::{scaffold_ruby, scaffold_ruby_cargo};
pub(crate) use wasm::scaffold_wasm;
