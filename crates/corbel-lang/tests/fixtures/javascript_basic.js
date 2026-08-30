export function pub(a, b) {
    return a + b;
}

function priv() {
    return 1;
}

export const arrowFn = (a) => {
    return a * 2;
};

const notExported = () => {
    return 0;
};

export class Foo extends Base {
    #secret = 1;

    constructor() {
        pub(1, 2);
    }

    pubMethod() {
        priv();
        return this.#secret;
    }

    #privMethod() {
        return this.#secret;
    }
}

export function caller() {
    pub(1, 2);
    arrowFn(3);
    return new Foo();
}
