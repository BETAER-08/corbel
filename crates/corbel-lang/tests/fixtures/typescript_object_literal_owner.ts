export const defaultProvider = {
    buildMessage(x: number): number {
        return x;
    },
};

const config = {
    handlers: {
        onEvent(x: number): number {
            return x;
        },
    },
};

function accept(obj: { run(): void }) {
    obj.run();
}

accept({
    run() {
        return;
    },
});
