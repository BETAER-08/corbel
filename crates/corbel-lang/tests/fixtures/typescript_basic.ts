export function pub(a: number, b: string): number {
    return a;
}

function priv() {
    return 1;
}

export const arrowFn = (a: number): number => {
    return a;
};

const notExported = () => 1;

export class Foo extends Bar implements Baz {
    public pubField: number;
    private privField: string;

    constructor(x: number) {
        this.pubField = x;
    }

    public pubMethod() {
        priv();
    }

    private privMethod() {
        return 1;
    }
}

export interface Shape {
    area(): number;
    name: string;
}

export type Point = {
    x: number;
    y: number;
};

export enum Color {
    Red,
    Green,
    Blue,
}

export function caller() {
    pub(1, "a");
    const f = new Foo(1);
    f.pubMethod();
    arrowFn(1);
}
