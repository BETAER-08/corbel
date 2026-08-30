import { Child } from "./Child";

export function Comp({ name }) {
    return (
        <div className="wrap">
            <Child name={name} />
            <Namespace.Item />
            <span>{name}</span>
        </div>
    );
}

export const Arrow = () => {
    return <Comp name="x" />;
};

function plain() {
    return helper();
}

function helper() {
    return 1;
}

export function caller() {
    plain();
    return <Comp name="y" />;
}
