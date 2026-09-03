pub struct Container<T> {
    value: T,
}

impl<T> Container<T> {
    pub fn get(&self) -> &T {
        &self.value
    }
}

pub struct Bar;

pub trait Foo {
    fn foo_method(&self);
}

impl Foo for &Bar {
    fn foo_method(&self) {}
}

pub mod scoped {
    pub struct Baz;
}

impl scoped::Baz {
    pub fn baz_method(&self) {}
}
