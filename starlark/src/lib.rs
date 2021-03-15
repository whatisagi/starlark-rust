/*
 * Copyright 2018 The Starlark in Rust Authors.
 * Copyright (c) Facebook, Inc. and its affiliates.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     https://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! A [Starlark interpreter in Rust](https://github.com/facebookexperimental/starlark-rust).
//! Starlark is a deterministic version of Python, with [a specification](https://github.com/bazelbuild/starlark/blob/master/spec.md),
//! used by (amongst others) the [Buck](https://buck.build) and [Bazel](https://bazel.build) systems.
//!
//! To evaluate a simple file:
//!
//! ```
//! # fn run() -> anyhow::Result<()> {
//! use starlark::eval::Evaluator;
//! use starlark::environment::{Module, Globals};
//! use starlark::values::Value;
//! use starlark::syntax::{AstModule, Dialect};
//!
//! let content = r#"
//! def hello():
//!    return "hello"
//! hello() + " world!"
//! "#;
//!
//! // We first parse the content, giving a filename and the Starlark
//! // `Dialect` we'd like to use (we pick standard).
//! let ast: AstModule = AstModule::parse("hello_world.star", content.to_owned(), &Dialect::Standard)?;
//!
//! // We create a `Globals`, defining the standard library functions available.
//! // By default, this uses those defined in the Starlark specification.
//! let globals: Globals = Globals::default();
//!
//! // We create a `Module`, which stores the global variables for our calculation.
//! let module: Module = Module::new();
//!
//! // We create an evaluator, which controls how evaluation occurs.
//! let mut eval: Evaluator = Evaluator::new(&module, &globals);
//!
//! // And finally we evaluate the code using the evaluator.
//! let res: Value = eval.eval_module(ast)?;
//! assert_eq!(res.unpack_str(), Some("hello world!"));
//! # Ok(())
//! # }
//! # fn main(){ run().unwrap(); }
//! ```
//!
//! From this example there are lots of ways to extend it, which we do so below.
//!
//! ## Call Rust functions from Starlark
//!
//! We want to define a function in Rust (that computes quadratics), and then call it from Starlark.
//!
//! ```
//! #[macro_use]
//! extern crate starlark_module;
//! # fn run() -> anyhow::Result<()> {
//! use starlark::environment::{GlobalsBuilder, Module};
//! use starlark::eval::Evaluator;
//! use starlark::syntax::{AstModule, Dialect};
//! use starlark::values::Value;
//!
//! let content = r#"
//! quadratic(4, 2, 1, x = 8)
//! "#;
//!
//! #[starlark_module]
//! fn starlark_quadratic(builder: &mut GlobalsBuilder) {
//!     // This defines a function that is visible to Starlark
//!     fn quadratic(a: i32, b: i32, c: i32, x: i32) -> i32 {
//!         Ok(a * x * x + b * x + c)
//!     }
//! }
//!
//! let ast = AstModule::parse("quadratic.star", content.to_owned(), &Dialect::Standard)?;
//! // We build our globals adding some functions we wrote
//! let globals = GlobalsBuilder::new().with(starlark_quadratic).build();
//! let module = Module::new();
//! let mut eval = Evaluator::new(&module, &globals);
//! let res = eval.eval_module(ast)?;
//! assert_eq!(res.unpack_int(), Some(273));
//! # Ok(())
//! # }
//! # fn main(){ run().unwrap(); }
//! ```
//!
//! ## Collect Starlark values
//!
//! If we want to use Starlark as an enhanced JSON, we can define an `emit` function
//! to "write out" a JSON value, and use the `Evaluator` extra fields to store it.
//!
//! ```
//! #[macro_use]
//! extern crate starlark_module;
//! # fn run() -> anyhow::Result<()> {
//! use starlark::environment::{GlobalsBuilder, Module};
//! use starlark::eval::Evaluator;
//! use starlark::syntax::{AstModule, Dialect};
//! use starlark::values::{none::NoneType, Value, ValueLike};
//! use gazebo::any::AnyLifetime;
//! use std::cell::RefCell;
//!
//! let content = r#"
//! emit(1)
//! emit(["test"])
//! emit({"x": "y"})
//! "#;
//!
//! // Define a store in which to accumulate JSON strings
//! #[derive(Debug, AnyLifetime, Default)]
//! struct Store(RefCell<Vec<String>>);
//!
//! impl Store {
//!     fn add(&self, x: String) {
//!          self.0.borrow_mut().push(x)
//!     }
//! }
//!
//! #[starlark_module]
//! fn starlark_emit(builder: &mut GlobalsBuilder) {
//!     fn emit(x: Value) -> NoneType {
//!         // We modify extra (which we know is a Store) and add the JSON of the
//!         // value the user gave.
//!         ctx.extra
//!             .unwrap()
//!             .downcast_ref::<Store>()
//!             .unwrap()
//!             .add(x.to_json());
//!         Ok(NoneType)
//!     }
//! }
//!
//! let ast = AstModule::parse("json.star", content.to_owned(), &Dialect::Standard)?;
//! // We build our globals adding some functions we wrote
//! let globals = GlobalsBuilder::new().with(starlark_emit).build();
//! let module = Module::new();
//! let mut eval = Evaluator::new(&module, &globals);
//! // We add a reference to our store
//! let store = Store::default();
//! eval.extra = Some(&store);
//! eval.eval_module(ast)?;
//! assert_eq!(&*store.0.borrow(), &["1", "[\"test\"]", "{\"x\": \"y\"}"]);
//! # Ok(())
//! # }
//! # fn main(){ run().unwrap(); }
//! ```
//!
//! ## Enable Starlark extensions (e.g. types)
//!
//! Our Starlark supports a number of extensions, including type annotations, which are
//! controlled by the `Dialect` type.
//!
//! ```
//! # fn run() -> anyhow::Result<()> {
//! use starlark::environment::{Globals, Module};
//! use starlark::eval::Evaluator;
//! use starlark::syntax::{AstModule, Dialect};
//!
//! let content = r#"
//! def takes_int(x: "int"):
//!     pass
//! takes_int("test")
//! "#;
//!
//! // Make the dialect enable types
//! let dialect = Dialect {enable_types: true, ..Dialect::Standard};
//! // We could equally have done `dialect = Dialect::Extended`.
//! let ast = AstModule::parse("json.star", content.to_owned(), &dialect)?;
//! let globals = Globals::default();
//! let module = Module::new();
//! let mut eval = Evaluator::new(&module, &globals);
//! let res = eval.eval_module(ast);
//! // We expect this to fail, since it is a type violation
//! assert!(res.unwrap_err().to_string().contains("Value `test` of type `string` does not match the type annotation `int`"));
//! # Ok(())
//! # }
//! # fn main(){ run().unwrap(); }
//! ```
//!
//! ## Enable the `load` statement
//!
//! You can have Starlark load files imported by the user. That requires that the loaded modules are first frozen.
//! There is no requirement that the files are on disk, but that would be a common pattern.
//!
//! ```
//! # fn run() -> anyhow::Result<()> {
//! use starlark::environment::{FrozenModule, Globals, Module};
//! use starlark::eval::{Evaluator, ReturnFileLoader};
//! use starlark::syntax::{AstModule, Dialect};
//!
//! // Get the file contents (for the demo), in reality use `AstModule::parse_file`.
//! fn get_source(file: &str) -> &str {
//!     match file {
//!         "a.star" => "a = 7",
//!         "b.star" => "b = 6",
//!         _ => { r#"
//! load('a.star', 'a')
//! load('b.star', 'b')
//! ab = a * b
//! "#
//!         }
//!     }
//! }
//!
//! fn get_module(file: &str) -> anyhow::Result<FrozenModule> {
//!    let ast = AstModule::parse(file, get_source(file).to_owned(), &Dialect::Standard)?;
//!
//!    // We can get the loaded modules from `ast.loads`.
//!    // And ultimately produce a `loader` capable of giving those modules to Starlark.
//!    let mut loads = Vec::new();
//!    for load in ast.loads() {
//!        loads.push((load.to_owned(), get_module(load)?));
//!    }
//!    let modules = loads.iter().map(|(a, b)| (a.as_str(), b)).collect();
//!    let mut loader = ReturnFileLoader { modules: &modules };
//!
//!    let globals = Globals::default();
//!    let module = Module::new();
//!    let mut eval = Evaluator::new(&module, &globals);
//!    eval.set_loader(&mut loader);
//!    eval.eval_module(ast)?;
//!    // After creating a module we freeze it, preventing further mutation.
//!    // It can now be used as the input for other Starlark modules.
//!    Ok(module.freeze())
//! }
//!
//! let ab = get_module("ab.star")?;
//! assert_eq!(ab.get("ab").unwrap().unpack_int(), Some(42));
//! # Ok(())
//! # }
//! # fn main(){ run().unwrap(); }
//! ```

// Features we use
#![feature(backtrace)]
#![feature(box_patterns)]
#![feature(box_syntax)]
#![feature(hash_set_entry)]
#![feature(try_blocks)]
//
// Plugins
#![cfg_attr(feature = "custom_linter", feature(plugin))]
#![cfg_attr(feature = "custom_linter", allow(deprecated))] // :(
#![cfg_attr(feature = "custom_linter", plugin(linter))]
//
// Good reasons
#![allow(clippy::new_ret_no_self)] // We often return Value, even though its morally a Self
#![allow(clippy::needless_return)] // Mixing explicit returns with implicit ones sometimes looks odd
// Disagree these are good hints
#![allow(clippy::single_match)]
#![allow(clippy::should_implement_trait)]
#![allow(clippy::len_without_is_empty)]
#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::new_without_default)]
#![allow(clippy::type_complexity)]
#![allow(clippy::needless_lifetimes)]
// FIXME: Temporary
#![allow(clippy::useless_transmute)] // Seems to be a clippy bug, but we should be using less transmute anyway

#[macro_use]
extern crate gazebo;
#[macro_use]
extern crate starlark_module;
#[macro_use]
extern crate maplit;

#[macro_use]
mod macros;

pub mod analysis;
pub mod assert;
pub mod collections;
pub mod debug;
pub mod environment;
pub mod errors;
pub mod eval;
pub mod stdlib;
pub mod syntax;
pub mod values;

/// __macro_refs allows us to reference other crates in macro rules without users needing to be
///  aware of those dependencies. We make them public here and then can reference them like
///  `$crate::__macro_refs::foo`.
#[doc(hidden)]
pub mod __macro_refs {
    pub use gazebo::any_lifetime;
    pub use paste::item;
}
