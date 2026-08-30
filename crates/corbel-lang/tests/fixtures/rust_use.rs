use a::b::c;
use a::b::c as d;
use a::b::{c, d, e};
use a::{b::c, d::{e, f}};
use a::b::{self, c};
use a::b::*;
use crate::foo::Bar;
use super::baz::Qux;
pub use a::b::c;
#[cfg(test)]
use a::b::testonly;

mod inner {
    use x::y::z;
}
