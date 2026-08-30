pub fn add(a: i32, b: i32) -> i32 {
    helper(a, b)
}

fn helper(a: i32, b: i32) -> i32 {
    a + b
}

pub struct Point {
    pub x: i32,
    y: i32,
}

pub trait Shape {
    fn area(&self) -> f64;
}

impl Point {
    pub fn new(x: i32, y: i32) -> Self {
        Point { x, y }
    }

    fn distance_from_origin(&self) -> f64 {
        ((self.x * self.x + self.y * self.y) as f64).sqrt()
    }
}
