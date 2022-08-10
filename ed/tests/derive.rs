use ed::{Decode, Encode};

#[derive(Encode, Decode)]
struct Foo {
    x: u32,
    y: (u32, u32),
}

#[derive(Encode, Decode)]
struct Foo2(u32, (u32, u32));

#[derive(Encode, Decode)]
struct Foo3;

#[derive(Encode, Decode)]
struct Foo4<T: Default>(T);

#[derive(Encode, Decode)]
enum Bar {
    A { x: u32, y: (u32, u32) },
    B(u32, (u32, u32)),
    C,
}

trait Subtype {
    type Subtype;
}

#[derive(Encode, Decode)]
enum Bar2<T: Subtype, U> {
    A { x: u32, y: (u32, u32) },
    B(u32, (u32, u32)),
    C,
    D(T::Subtype, U),
}
