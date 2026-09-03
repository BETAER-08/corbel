export function freeFn() {
    return 1;
}

export class Widget {
    render() {
        return freeFn();
    }
}
